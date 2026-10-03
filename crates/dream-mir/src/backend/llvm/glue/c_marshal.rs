//! The `@c` boundary, Dream side. A forward trampoline turns Dream values into what the generated
//! C shim takes (see `dream_abi::c_abi::shim`) by each parameter's [`CShape`] and turns the shim's
//! result back. Scalars and by-value structs cross in Dream's own representation (registers and
//! struct addresses); the shim, compiled by clang, owns the platform C ABI. Strings, `CPtr`s,
//! arrays and callbacks are converted here.

use super::super::fx::{Fx, V};
use super::super::ir::{FnTy, Ty, Value};
use super::super::lcx::{FnSig, Lcx};
use super::super::types::{is_unsigned, ll_ty};
use super::c_reverse::{callback_fun_ty, direct_targets, Reverse};
use super::{glue, register};
use crate::backend::shared::abi_types::{c_ident, elem_size, import_call_name};
use crate::backend::shared::panic_msgs;
use dream_abi::c_abi::shim;
use dream_hir::{CShape, HImport};
use dream_types::TypeId;

pub(super) fn shape(imp: &HImport, i: usize) -> &CShape {
    &imp.c_params[i]
}

/// The shim function a forward trampoline calls.
pub(super) fn shim_name(imp: &HImport) -> String {
    shim::forward_name(&c_ident(&imp.name))
}

/// The shim function returning the address of an `OwnedCPtr` result's C destructor.
pub(super) fn destructor_getter(imp: &HImport) -> String {
    shim::destructor_getter_name(&shim_name(imp))
}

fn ret_ty(l: &Lcx<'_>, imp: &HImport) -> TypeId {
    imp.ret.unwrap_or_else(|| l.interner.int())
}

/// The shim's parameter types for Dream argument `i`.
fn shim_param_tys(l: &Lcx<'_>, imp: &HImport, i: usize) -> Vec<Ty> {
    match shape(imp, i) {
        CShape::Callback { .. } => vec![Ty::Ptr, Ty::Ptr],
        CShape::Scalar(_) => vec![ll_ty(l.interner, imp.params[i], &l.h(), &l.word())],
        _ => vec![Ty::Ptr],
    }
}

/// Declares the shim and the Dream-callable trampoline for a `@c` import.
pub(super) fn register_import(l: &mut Lcx<'_>, imp: &HImport) {
    let mut abi: Vec<Ty> = (0..imp.params.len())
        .flat_map(|i| shim_param_tys(l, imp, i))
        .collect();
    let ret = match &imp.c_ret {
        CShape::Void => Ty::Void,
        CShape::Scalar(_) => ll_ty(l.interner, ret_ty(l, imp), &l.h(), &l.word()),
        CShape::Struct => {
            abi.insert(0, Ty::Ptr);
            Ty::Void
        }
        _ => Ty::Ptr,
    };
    l.host(&shim_name(imp), FnSig::plain(FnTy::new(ret, abi)));
    if let CShape::OwnedPtr { .. } = imp.c_ret {
        l.host(
            &destructor_getter(imp),
            FnSig::plain(FnTy::new(Ty::Ptr, vec![])),
        );
    }
    l.host("free", FnSig::plain(FnTy::new(Ty::Void, vec![Ty::Ptr])));
    let params = imp
        .params
        .iter()
        .enumerate()
        .map(|(i, t)| {
            if imp.param_by_ref.get(i).copied().unwrap_or(false) {
                l.h()
            } else {
                ll_ty(l.interner, *t, &l.h(), &l.word())
            }
        })
        .collect();
    let dream_ret = imp
        .ret
        .map(|t| ll_ty(l.interner, t, &l.h(), &l.word()))
        .unwrap_or(Ty::Void);
    register(l, &import_call_name(imp), dream_ret, params);
}

pub(super) fn trampoline(l: &mut Lcx<'_>, imp: &HImport) {
    let shim = shim_name(imp);
    let wrap = import_call_name(imp);
    let mut fx = glue(l, &wrap);
    let mut args = Vec::new();
    let mut frees = Vec::new();
    let out = match imp.c_ret {
        CShape::Struct => {
            let b = fx.struct_box(ret_ty(fx.l, imp));
            args.push(V::s(fx.ptr(&b)));
            Some(b)
        }
        _ => None,
    };
    for (i, ty) in imp.params.iter().enumerate() {
        let a = V {
            v: fx.w.param(i),
            unsigned: is_unsigned(fx.interner, *ty),
        };
        fx.c_arg(imp, i, *ty, &a, &mut args, &mut frees);
    }
    let r = fx.call(&shim, &args);
    for f in frees {
        fx.call("free", &[f]);
    }
    let r = match out {
        Some(b) => Some(b),
        None => r.map(|r| fx.c_result(imp, &r)),
    };
    match r {
        Some(r) if !fx.w.ret.is_void() => {
            let t = fx.w.ret.clone();
            let r = fx.conv(&r, &t);
            fx.w.ret(Some(&r));
        }
        _ => fx.w.ret(None),
    }
    fx.finish();
}

impl<'l, 'a> Fx<'l, 'a> {
    fn is_zero(&mut self, a: &V) -> Value {
        self.w.icmp("eq", &a.v, &Value::zero(a.ty().clone()))
    }

    /// `a == 0 ? NULL : present(a)`.
    fn null_or(&mut self, a: &V, present: impl FnOnce(&mut Self) -> V) -> V {
        let z = self.is_zero(a);
        self.if_else_v(&z, |_| V::s(Value::null()), present)
    }

    /// The `raw` word of the `CPtr` stored at `at`, as `void*`.
    fn cptr_load(&mut self, at: &Value) -> V {
        let raw = self.load_ty(
            self.l.word(),
            at,
            self.l.cx.mir.layouts.target.ptr_align,
            true,
        );
        V::s(self.ptr(&raw))
    }

    fn c_arg(
        &mut self,
        imp: &HImport,
        i: usize,
        ty: TypeId,
        a: &V,
        args: &mut Vec<V>,
        frees: &mut Vec<V>,
    ) {
        match shape(imp, i).clone() {
            CShape::Void | CShape::Scalar(_) => args.push(a.clone()),
            CShape::Ref | CShape::Struct | CShape::OwnedPtr { .. } => args.push(V::s(self.ptr(a))),
            CShape::Str { optional } => {
                let conv = if imp.c_wide_strings {
                    "dream_string_to_utf16z"
                } else {
                    "dream_string_to_utf8"
                };
                let s = if optional {
                    self.null_or(a, |fx| {
                        let s = fx.call_v(conv, std::slice::from_ref(a));
                        V::s(fx.ptr(&s))
                    })
                } else {
                    let s = self.call_v(conv, std::slice::from_ref(a));
                    V::s(self.ptr(&s))
                };
                frees.push(s.clone());
                args.push(s);
            }
            CShape::Ptr { optional: false } => {
                let at = self.ptr(a);
                let p = self.cptr_load(&at);
                args.push(p);
            }
            CShape::Ptr { optional: true } => {
                let (none, off) = self.option_layout(ty);
                let disc = {
                    let p = self.ptr(a);
                    self.load_ty(Ty::I32, &p, 4, false)
                };
                let is_none = self.w.icmp("eq", &disc.v, &Value::i32(none as i64));
                let p = self.if_else_v(
                    &is_none,
                    |_| V::s(Value::null()),
                    |fx| {
                        let at = fx.addr(a, off as i64);
                        fx.cptr_load(&at)
                    },
                );
                args.push(p);
            }
            s @ CShape::Func { optional, .. } => {
                let name = imp.name.clone();
                let fun_ty = callback_fun_ty(self.l, ty);
                let fp = if optional {
                    self.null_or(a, |fx| fx.fun_to_c(a, &s, fun_ty, &name))
                } else {
                    self.fun_to_c(a, &s, fun_ty, &name)
                };
                args.push(fp);
            }
            CShape::Callback {
                user_data_last,
                optional,
                ..
            } => {
                let rev = Reverse::Callback {
                    fun_ty: callback_fun_ty(self.l, ty),
                    user_data_last,
                };
                let tramp = V::s(self.l.fn_ref(&rev.symbol()));
                let (fp, ud) = if optional {
                    let fp = self.null_or(a, |_| tramp.clone());
                    (fp, V::s(self.ptr(a)))
                } else {
                    (tramp, V::s(self.ptr(a)))
                };
                args.push(fp);
                args.push(ud);
            }
            CShape::Array => {
                let data =
                    self.null_or(a, |fx| V::s(fx.addr(a, crate::abi::LEN_PREFIX_SIZE as i64)));
                args.push(data);
            }
        }
    }

    /// A C function pointer for the funcbox `a`: the Dream function itself when the signatures
    /// agree, else the per-target adapter selected by its function-table index.
    fn fun_to_c(&mut self, a: &V, s: &CShape, fun_ty: TypeId, import: &str) -> V {
        let env = self.call_v("dream_funcbox_env", std::slice::from_ref(a));
        let captures = self.w.icmp("ne", &env.v, &Value::zero(env.ty().clone()));
        let msg = panic_msgs::c_capturing_closure(import);
        self.if_then(&captures, |fx| fx.panic_with(&msg));
        let idx = self.call_v("dream_funcbox_funcidx", std::slice::from_ref(a));
        if !s.needs_wrapper() {
            let f = self.call_v("dream_ft_get", &[idx]);
            return V::s(self.ptr(&f));
        }
        let mut fp = Value::null();
        for f in direct_targets(self.l, fun_ty) {
            let rev = Reverse::Direct {
                symbol: self.l.abi_sym(&self.l.user_fn(f)),
                fun_ty,
            };
            let slot = self.l.cx.func_index(f) as i64;
            let hit = self.w.icmp("eq", &idx.v, &Value::i32(slot));
            let wrapper = self.l.fn_ref(&rev.symbol());
            fp = self.w.select(&hit, &wrapper, &fp);
        }
        let fp = V::s(fp);
        let missing = self.is_zero(&fp);
        let msg = panic_msgs::c_no_direct_target(import);
        self.if_then(&missing, |fx| fx.panic_with(&msg));
        fp
    }

    /// `(None discriminant, Some payload offset)` of an `Option<CPtr>`.
    fn option_layout(&self, ty: TypeId) -> (i32, u32) {
        let u = self
            .l
            .cx
            .nunion(ty)
            .unwrap_or_else(|| crate::internal_error!("Option<CPtr> without a union layout"));
        let none = u
            .variant("None")
            .unwrap_or_else(|| crate::internal_error!("Option without None"))
            .discriminant;
        let off = u
            .variant("Some")
            .and_then(|v| v.fields.first())
            .unwrap_or_else(|| crate::internal_error!("Option without a Some payload"))
            .offset;
        (none, off)
    }

    /// A fresh `Option<CPtr>` envelope holding `p` (`None` for `NULL`).
    pub(super) fn option_cptr_new(&mut self, ty: TypeId, p: &V) -> V {
        let u = self
            .l
            .cx
            .nunion(ty)
            .unwrap_or_else(|| crate::internal_error!("Option<CPtr> without a union layout"));
        let size = u.size as i64;
        let some = u
            .variant("Some")
            .unwrap_or_else(|| crate::internal_error!("Option without Some"))
            .discriminant;
        let (none, off) = self.option_layout(ty);
        let tag = self.l.cx.type_tag(ty) as i64;
        let b = self.call_v("dream_malloc", &[V::i64(size), V::i32(tag)]);
        let bp = self.ptr(&b);
        self.memset0(&bp, &Value::i64(size));
        let z = self.is_zero(p);
        let disc = self
            .w
            .select(&z, &Value::i32(none as i64), &Value::i32(some as i64));
        self.store_ty(&Ty::I32, &bp, &V::s(disc), 4);
        let at = self.addr(&b, off as i64);
        let raw = V::u(self.conv(p, &self.l.word()));
        self.store_ty(
            &self.l.word(),
            &at,
            &raw,
            self.l.cx.mir.layouts.target.ptr_align,
        );
        self.as_ref(&b)
    }

    /// A fresh heap box for a value struct of type `ty`, the value-struct result convention for
    /// native calls.
    fn struct_box(&mut self, ty: TypeId) -> V {
        let size = elem_size(&self.l.cx, ty) as i64;
        let tag = self.l.cx.type_tag(ty) as i64;
        let b = self.call_v("dream_malloc", &[V::i64(size), V::i32(tag)]);
        self.as_ref(&b)
    }

    /// Stores the pointer `p` as the `raw` word of the `CPtr` at `at`.
    fn cptr_store(&mut self, at: &Value, p: &V) {
        let raw = V::u(self.conv(p, &self.l.word()));
        self.store_ty(
            &self.l.word(),
            at,
            &raw,
            self.l.cx.mir.layouts.target.ptr_align,
        );
    }

    /// A fresh heap `CPtr` holding `p`.
    fn cptr_new(&mut self, ty: TypeId, p: &V) -> V {
        let b = self.struct_box(ty);
        let bp = self.ptr(&b);
        self.cptr_store(&bp, p);
        b
    }

    /// A fresh `OwnedCPtr` (fields `ptr: CPtr`, `free: usize`) owning `p`, freed by `free`.
    fn owned_cptr_new(&mut self, ty: TypeId, p: &V, free: &V) -> V {
        let offsets: Vec<u32> = self
            .l
            .cx
            .nstruct(ty)
            .unwrap_or_else(|| crate::internal_error!("OwnedCPtr without a layout"))
            .fields
            .iter()
            .map(|f| f.offset)
            .collect();
        let [ptr_off, free_off] = offsets[..] else {
            crate::internal_error!("OwnedCPtr must have exactly the fields `ptr` and `free`");
        };
        let o = self.emit_new_in(ty, None, &[], None);
        let at = self.addr(&o, ptr_off as i64);
        self.cptr_store(&at, p);
        let at = self.addr(&o, free_off as i64);
        self.cptr_store(&at, free);
        o
    }

    /// A Dream `string` from a C string; `NULL` traps with `what` unless `optional`.
    pub(super) fn c_string(&mut self, p: &V, optional: bool, what: &str) -> V {
        let z = self.is_zero(p);
        if optional {
            let h = self.h();
            return self.if_else_v(
                &z,
                |_| V::u(Value::zero(h)),
                |fx| fx.call_v("dream_utf8_to_string", std::slice::from_ref(p)),
            );
        }
        let msg = panic_msgs::c_null_string(what);
        self.if_then(&z, |fx| fx.panic_with(&msg));
        self.call_v("dream_utf8_to_string", std::slice::from_ref(p))
    }

    fn c_result(&mut self, imp: &HImport, r: &V) -> V {
        let ret_ty = ret_ty(self.l, imp);
        match imp.c_ret.clone() {
            CShape::Str { optional } => {
                let what = panic_msgs::c_result_what(&imp.name);
                self.c_string(r, optional, &what)
            }
            CShape::Ptr { optional: false } => self.cptr_new(ret_ty, r),
            CShape::Ptr { optional: true } => self.option_cptr_new(ret_ty, r),
            CShape::OwnedPtr { .. } => {
                let free = self.call_v(&destructor_getter(imp), &[]);
                self.owned_cptr_new(ret_ty, r, &free)
            }
            _ => r.clone(),
        }
    }
}
