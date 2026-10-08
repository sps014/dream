//! Per-function lowering state and the value plumbing every writer shares: C-compatible
//! conversions, address formation, loads/stores, runtime calls and small control-flow helpers.
//!
//! Native references retain LLVM pointer provenance; linear-memory references are offsets.
//! Integer conversions are restricted to explicit raw-address and untyped payload boundaries.

use super::ir::{BlockRef, CallArg, CallConv, FnTy, FunctionWriter, MdRef, Tail, Ty, Value};
use super::lcx::{FnSig, Lcx};
use crate::backend::shared::abi_types::MemTy;
use crate::{Mir, MirFunction};
use dream_types::TypeInterner;

/// An SSA value plus the signedness C would give its type, which decides how it widens.
#[derive(Clone, Debug)]
pub(super) struct V {
    pub v: Value,
    pub unsigned: bool,
}

impl V {
    pub fn s(v: Value) -> Self {
        Self { v, unsigned: false }
    }

    pub fn u(v: Value) -> Self {
        Self { v, unsigned: true }
    }

    pub fn ty(&self) -> &Ty {
        &self.v.ty
    }

    pub fn i32(n: i64) -> Self {
        Self::s(Value::i32(n))
    }

    pub fn i64(n: i64) -> Self {
        Self::s(Value::i64(n))
    }
}

pub(super) struct Slot {
    pub ptr: Value,
    pub ty: Ty,
    pub unsigned: bool,
}

pub(super) struct Fx<'l, 'a> {
    pub l: &'l mut Lcx<'a>,
    pub mir: &'a Mir,
    pub interner: &'a TypeInterner,
    pub f: &'a MirFunction,
    pub w: FunctionWriter,
    pub slots: Vec<Option<Slot>>,
    pub blocks: Vec<BlockRef>,
    pub wide: Vec<bool>,
    pub frame_bufs: Vec<Option<Value>>,
    pub value_frame: crate::backend::shared::ValueFrame,
    /// The poll function's `__self` future (as `dream_ptr`); `None` in sync bodies.
    pub self_: Option<Value>,
    pub poll_offsets: Vec<i32>,
    pub poll_environment: Option<i32>,
    /// Constructor proof guarantees fresh, zeroed fields with one write each.
    pub fresh_init: bool,
    /// The subprogram's file and first line, when this body carries debug info.
    pub dbg: Option<(MdRef, u32)>,
    /// Source file panic locations name (`source_loc.rs`); `None` outside user bodies.
    pub src_file: Option<String>,
    /// The source line in effect on entry to each block.
    pub src_lines: Vec<Option<u32>>,
    /// The source line in effect at the statement being emitted.
    pub src_line: Option<u32>,
    /// A caller-tracking body's incoming caller location (its trailing hidden parameter).
    pub caller_loc: Option<Value>,
}

pub(super) fn mem_ll(m: MemTy, h: &Ty, word: &Ty) -> (Ty, bool) {
    match m {
        MemTy::U8 => (Ty::I8, true),
        MemTy::I32 => (Ty::I32, false),
        MemTy::I64 => (Ty::I64, false),
        MemTy::Word => (word.clone(), false),
        MemTy::UWord => (word.clone(), true),
        MemTy::F32 => (Ty::F32, false),
        MemTy::F64 => (Ty::F64, false),
        MemTy::Ptr => (h.clone(), true),
    }
}

fn natural_align(ty: &Ty) -> u32 {
    match ty {
        Ty::Int(b) => (b / 8).clamp(1, 8),
        Ty::F32 => 4,
        Ty::F64 | Ty::Ptr => 8,
        _ => 1,
    }
}

/// The alignment a byte offset from an 8-aligned base guarantees for an access of `ty`.
pub(super) fn align_at(ty: &Ty, off: i64) -> u32 {
    let low = if off == 0 {
        8
    } else {
        1u32 << (off.trailing_zeros().min(3))
    };
    natural_align(ty).min(low)
}

impl<'l, 'a> Fx<'l, 'a> {
    pub fn new(l: &'l mut Lcx<'a>, f: &'a MirFunction, w: FunctionWriter) -> Self {
        let mir = l.mir;
        let interner = l.interner;
        let wide = crate::backend::shared::abi_types::ref_int_locals(&l.cx, f);
        Self {
            l,
            mir,
            interner,
            f,
            w,
            slots: Vec::new(),
            blocks: Vec::new(),
            wide,
            frame_bufs: Vec::new(),
            value_frame: crate::backend::shared::ValueFrame::compute(f, interner),
            self_: None,
            poll_offsets: Vec::new(),
            poll_environment: None,
            fresh_init: false,
            dbg: None,
            src_file: None,
            src_lines: Vec::new(),
            src_line: None,
            caller_loc: None,
        }
    }

    /// Hands the finished function to the module.
    pub fn finish(self) {
        let Fx { l, w, .. } = self;
        l.define(w);
    }

    // ---- conversions --------------------------------------------------------------------------

    /// C's implicit conversion of `x` to `to` (the width change follows `x`'s signedness).
    pub fn conv(&mut self, x: &V, to: &Ty) -> Value {
        let from = x.ty().clone();
        if &from == to {
            return x.v.clone();
        }
        match (&from, to) {
            (Ty::Int(a), Ty::Int(b)) => {
                if let Some(c) = x.v.const_int() {
                    return Value::int(to.clone(), wrap_const(c, *a, x.unsigned || *a == 1, *b));
                }
                if a < b {
                    let op = if x.unsigned || *a == 1 {
                        "zext"
                    } else {
                        "sext"
                    };
                    self.w.cast(op, &x.v, to.clone())
                } else {
                    self.w.cast("trunc", &x.v, to.clone())
                }
            }
            (Ty::Int(a), Ty::F32 | Ty::F64) => {
                let op = if x.unsigned || *a == 1 {
                    "uitofp"
                } else {
                    "sitofp"
                };
                self.w.cast(op, &x.v, to.clone())
            }
            (Ty::F32 | Ty::F64, Ty::Int(b)) => {
                let name = format!("llvm.fptosi.sat.i{b}.{}", float_suffix(&from));
                let sig = self
                    .l
                    .intrinsic(&name, FnTy::new(to.clone(), vec![from.clone()]));
                self.call_sig(&name, &sig, vec![x.v.clone()])
                    .unwrap_or_else(|| crate::internal_error!("fptosi.sat returned void"))
            }
            (Ty::F32, Ty::F64) => self.w.cast("fpext", &x.v, Ty::F64),
            (Ty::F64, Ty::F32) => self.w.cast("fptrunc", &x.v, Ty::F32),
            (Ty::Ptr, Ty::Int(64)) => self.w.cast("ptrtoint", &x.v, Ty::I64),
            (Ty::Ptr, Ty::Int(_)) => {
                let wide = self.w.cast("ptrtoint", &x.v, Ty::I64);
                self.w.cast("trunc", &wide, to.clone())
            }
            (Ty::Int(_), Ty::Ptr) => {
                let wide = self.conv(x, &Ty::I64);
                self.w.cast("inttoptr", &wide, Ty::Ptr)
            }
            _ => crate::internal_error!("no LLVM conversion from {from} to {to}"),
        }
    }

    pub fn conv_v(&mut self, x: &V, to: &Ty, unsigned: bool) -> V {
        V {
            v: self.conv(x, to),
            unsigned,
        }
    }

    /// A `dream_ptr` (or any integer) as an address.
    pub fn ptr(&mut self, x: &V) -> Value {
        self.conv(x, &Ty::Ptr)
    }

    pub fn h(&self) -> Ty {
        self.l.h()
    }

    pub fn word(&self) -> Ty {
        self.l.word()
    }

    /// The value as `dream_ptr` bits.
    pub fn as_ref(&mut self, x: &V) -> V {
        let h = self.h();
        V::u(self.conv(x, &h))
    }

    /// C truthiness (`x != 0`).
    pub fn truthy(&mut self, x: &V) -> Value {
        match x.ty() {
            Ty::Int(1) => x.v.clone(),
            Ty::Int(_) => self.w.icmp("ne", &x.v, &Value::zero(x.ty().clone())),
            Ty::F32 | Ty::F64 => self.w.fcmp("une", &x.v, &Value::zero(x.ty().clone())),
            Ty::Ptr => self.w.icmp("ne", &x.v, &Value::null()),
            other => crate::internal_error!("truthiness of {other}"),
        }
    }

    // ---- memory -------------------------------------------------------------------------------

    /// `(char *)dream_p(base) + off`.
    pub fn addr(&mut self, base: &V, off: i64) -> Value {
        let p = self.ptr(base);
        self.w.gep_const(&p, off)
    }

    pub fn addr_dyn(&mut self, base: &V, off: &Value) -> Value {
        let p = self.ptr(base);
        self.w.gep_i8(&p, off)
    }

    pub fn load_mem(&mut self, m: MemTy, ptr: &Value, align: u32) -> V {
        let (ty, unsigned) = mem_ll(m, &self.h(), &self.word());
        let a = align.min(natural_align(&ty));
        V {
            v: self.w.load(ty, ptr, a, &[]),
            unsigned,
        }
    }

    pub fn store_mem(&mut self, m: MemTy, ptr: &Value, x: &V, align: u32) {
        let (ty, _) = mem_ll(m, &self.h(), &self.word());
        let a = align.min(natural_align(&ty));
        let v = self.conv(x, &ty);
        self.w.store(&v, ptr, a, &[]);
    }

    pub fn load_ty(&mut self, ty: Ty, ptr: &Value, align: u32, unsigned: bool) -> V {
        let a = align.min(natural_align(&ty));
        V {
            v: self.w.load(ty, ptr, a, &[]),
            unsigned,
        }
    }

    pub fn store_ty(&mut self, ty: &Ty, ptr: &Value, x: &V, align: u32) {
        let a = align.min(natural_align(ty));
        let v = self.conv(x, ty);
        self.w.store(&v, ptr, a, &[]);
    }

    pub fn memcpy(&mut self, dst: &Value, src: &Value, n: &Value) {
        let n = self.conv(&V::s(n.clone()), &Ty::I64);
        let sig = self.l.intrinsic(
            "llvm.memcpy.p0.p0.i64",
            FnTy::new(Ty::Void, vec![Ty::Ptr, Ty::Ptr, Ty::I64, Ty::I1]),
        );
        self.call_sig(
            "llvm.memcpy.p0.p0.i64",
            &sig,
            vec![dst.clone(), src.clone(), n, Value::i1(false)],
        );
    }

    pub fn memset0(&mut self, dst: &Value, n: &Value) {
        let n = self.conv(&V::s(n.clone()), &Ty::I64);
        let sig = self.l.intrinsic(
            "llvm.memset.p0.i64",
            FnTy::new(Ty::Void, vec![Ty::Ptr, Ty::I8, Ty::I64, Ty::I1]),
        );
        self.call_sig(
            "llvm.memset.p0.i64",
            &sig,
            vec![dst.clone(), Value::int(Ty::I8, 0), n, Value::i1(false)],
        );
    }

    // ---- calls --------------------------------------------------------------------------------

    fn call_sig(&mut self, name: &str, sig: &FnSig, args: Vec<Value>) -> Option<Value> {
        let callee = Value::global(name);
        self.call_ptr(&callee, sig, args)
    }

    /// Calls `callee` (a symbol or a function pointer) typed by `sig`. `args` must already have
    /// the parameter types.
    pub fn call_ptr(&mut self, callee: &Value, sig: &FnSig, args: Vec<Value>) -> Option<Value> {
        let args: Vec<CallArg> = args
            .into_iter()
            .enumerate()
            .map(|(i, value)| CallArg {
                value,
                attrs: sig.param_attrs.get(i).cloned().unwrap_or_default(),
            })
            .collect();
        let r = self
            .w
            .call(Tail::None, CallConv::C, &sig.fty, callee, args, &[]);
        if sig.noreturn {
            self.w.unreachable();
        }
        r
    }

    /// Converts `args` to `sig`'s parameter types the way a C call through a prototype would.
    pub fn coerce_args(&mut self, sig: &FnSig, args: &[V]) -> Vec<Value> {
        if args.len() != sig.fty.params.len() && !sig.fty.varargs {
            crate::internal_error!(
                "call arity {} does not match LLVM signature {}",
                args.len(),
                sig.fty
            );
        }
        args.iter()
            .enumerate()
            .map(|(i, a)| match sig.fty.params.get(i) {
                Some(t) => {
                    let t = t.clone();
                    self.conv(a, &t)
                }
                None => a.v.clone(),
            })
            .collect()
    }

    /// Calls a named function (generated, host or runtime) with C argument conversions.
    pub fn call(&mut self, name: &str, args: &[V]) -> Option<V> {
        let sig = self.l.sig(name);
        let mut tracked;
        let args = if self.l.tracked.contains(name) {
            tracked = args.to_vec();
            tracked.push(self.panic_location());
            &tracked[..]
        } else {
            args
        };
        if args.len() != sig.fty.params.len() && !sig.fty.varargs {
            crate::internal_error!(
                "call to `{name}` passes {} arguments; its LLVM signature is {}",
                args.len(),
                sig.fty
            );
        }
        let vals = self.coerce_args(&sig, args);
        let unsigned = sig.ret_unsigned();
        self.call_sig(name, &sig, vals).map(|v| V { v, unsigned })
    }

    pub fn call_v(&mut self, name: &str, args: &[V]) -> V {
        self.call(name, args).unwrap_or_else(|| {
            crate::internal_error!("`{name}` returns void where a value is used")
        })
    }

    pub fn panic_with(&mut self, msg: &str) {
        let m = V::u(self.l.str_val(msg));
        let at = self.panic_location();
        self.call("dream_panic_at", &[m, at]);
    }

    pub fn str_v(&self, s: &str) -> V {
        V::u(self.l.str_val(s))
    }

    // ---- control ------------------------------------------------------------------------------

    /// `if (cond) { body }`.
    pub fn if_then(&mut self, cond: &Value, body: impl FnOnce(&mut Self)) {
        let then_b = self.w.new_block("then");
        let join = self.w.new_block("join");
        self.w.cond_br(cond, then_b, join);
        self.w.switch_to(then_b);
        body(self);
        if !self.w.is_terminated() {
            self.w.br(join);
        }
        self.w.switch_to(join);
    }

    /// `cond ? then() : else_()`, both producing values of the same type.
    pub fn if_else_v(
        &mut self,
        cond: &Value,
        then_f: impl FnOnce(&mut Self) -> V,
        else_f: impl FnOnce(&mut Self) -> V,
    ) -> V {
        let then_b = self.w.new_block("then");
        let else_b = self.w.new_block("else");
        let join = self.w.new_block("join");
        self.w.cond_br(cond, then_b, else_b);
        self.w.switch_to(then_b);
        let a = then_f(self);
        let a_end = self.w.current();
        let a_live = !self.w.is_terminated();
        if a_live {
            self.w.br(join);
        }
        self.w.switch_to(else_b);
        let b = else_f(self);
        let b_live = !self.w.is_terminated();
        let (b, b_end) = if b_live {
            let ty = a.ty().clone();
            let bv = self.conv(&b, &ty);
            let end = self.w.current();
            self.w.br(join);
            (bv, end)
        } else {
            (b.v, self.w.current())
        };
        self.w.switch_to(join);
        match (a_live, b_live) {
            (true, true) => {
                let phi = self
                    .w
                    .phi(a.ty().clone(), &[(a.v.clone(), a_end), (b, b_end)]);
                V {
                    v: phi,
                    unsigned: a.unsigned,
                }
            }
            (true, false) => a,
            (false, true) => V {
                v: b,
                unsigned: a.unsigned,
            },
            (false, false) => {
                self.w.unreachable();
                a
            }
        }
    }

    pub fn alloca_bytes(&mut self, size: u64, align: u32) -> Value {
        self.w.alloca(Ty::bytes(size.max(1)), align)
    }
}

fn float_suffix(t: &Ty) -> &'static str {
    match t {
        Ty::F32 => "f32",
        _ => "f64",
    }
}

/// Folds a constant's width change the same way the instruction would.
fn wrap_const(c: i128, from: u32, unsigned: bool, to: u32) -> i128 {
    let masked = if from >= 128 {
        c
    } else {
        c & ((1i128 << from) - 1)
    };
    let widened = if !unsigned && from < 128 && masked >> (from - 1) & 1 == 1 {
        masked - (1i128 << from)
    } else {
        masked
    };
    if to >= 128 {
        return widened;
    }
    let t = widened & ((1i128 << to) - 1);
    if to > 1 && t >> (to - 1) & 1 == 1 {
        t - (1i128 << to)
    } else {
        t
    }
}
