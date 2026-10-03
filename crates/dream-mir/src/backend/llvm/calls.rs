//! Direct, indirect (function table) and interface calls.

use super::fx::{Fx, V};
use super::ir::{Ty, Value};
use super::types::fn_ptr_sig;
use crate::backend::shared::abi_types::{c_ident, elem_size, fn_sig, runtime_c_name};
use crate::backend::shared::glue::retain_sym;
use crate::{Callee, Const, Operand, Place};
use dream_abi::intrinsics::IntrinsicOp;
use dream_types::{TyKind, TypeId};

impl<'l, 'a> Fx<'l, 'a> {
    pub fn call_expr(&mut self, callee: &Callee, args: &[Operand]) -> Option<V> {
        let raw = self.l.cx.callee_sym(callee.def, &callee.args);
        let name = runtime_c_name(&raw);
        if name == "dream_sb_push" && self.sb_push_units(args) {
            return None;
        }
        let mut vals = Vec::with_capacity(args.len() + 1);
        for (i, a) in args.iter().enumerate() {
            self.retain_rc_global_sink(callee.take_params.get(i).copied().unwrap_or(false), a);
            vals.push(self.operand(a));
        }
        if name == "dream_all" {
            let es = match self.interner.kind(callee.ret) {
                TyKind::Array(e) => elem_size(&self.l.cx, *e),
                _ => callee
                    .args
                    .first()
                    .map(|t| elem_size(&self.l.cx, *t))
                    .unwrap_or(4),
            };
            vals.push(V::i32(es as i64));
        }
        if IntrinsicOp::from_key(&raw) == Some(IntrinsicOp::Panic) {
            let at = self.panic_location();
            vals.push(at);
            return self.call("dream_panic_at", &vals);
        }
        if self.l.sret.contains(&name) {
            let size = elem_size(&self.l.cx, callee.ret) as u64;
            let buf = self.alloca_bytes(size, 16);
            vals.push(V::s(buf.clone()));
            self.call(&name, &vals);
            return Some(self.as_ref(&V::s(buf)));
        }
        self.call(&name, &vals)
    }

    /// The call's value-struct result sits in a caller buffer, not a heap box to free.
    pub fn is_buffer_result(&self, rv: &crate::Rvalue) -> bool {
        let crate::Rvalue::Call { callee, .. } = rv else {
            return false;
        };
        let raw = self.l.cx.callee_sym(callee.def, &callee.args);
        self.l.sret.contains(&runtime_c_name(&raw))
    }

    /// A `take` argument read straight from an RC global hands the callee its own reference.
    pub fn retain_rc_global_sink(&mut self, take: bool, arg: &Operand) {
        if !take {
            return;
        }
        let Operand::Copy(Place::Global(g)) = arg else {
            return;
        };
        let Some(ty) = self.l.cx.global_ty(*g) else {
            return;
        };
        if !self.is_rc(ty) {
            return;
        }
        let v = self.operand(arg);
        let sym = retain_sym(&self.l.cx, ty);
        self.call(sym, &[v]);
    }

    fn sb_push_units(&mut self, args: &[Operand]) -> bool {
        let Some(Operand::Const(Const::Str(s))) = args.get(1) else {
            return false;
        };
        let n = s.encode_utf16().count() as i64;
        if n <= 0 {
            return false;
        }
        let sb = self.operand(&args[0]);
        let lit = self.str_v(s);
        let units = self.addr(&lit, crate::abi::STRING_UNITS_OFFSET as i64);
        self.call("dream_sb_push_units", &[sb, V::s(units), V::i32(n)]);
        true
    }

    /// The function pointer in `dream_ft[idx]`.
    pub fn ft_entry(&mut self, idx: &V) -> Value {
        let i = self.conv_v(idx, &Ty::I32, false);
        let word = self.word();
        let i = self.conv(&i, &word);
        let ps = self.l.cx.target.abi().ptr_size as i64;
        let off = self.w.bin("mul", &i, &Value::int(word, ps as i128));
        let at = self.w.gep_i8(&Value::global("dream_ft"), &off);
        self.w.load(Ty::Ptr, &at, ps as u32, &[])
    }

    pub fn indirect_expr(&mut self, target: &Operand, args: &[Operand], sig: TypeId) -> Option<V> {
        let mut vals = Vec::with_capacity(args.len());
        for a in args {
            self.retain_rc_global_sink(true, a);
            vals.push(self.operand(a));
        }
        let s = fn_ptr_sig(self.interner, sig, &self.h(), &self.word());
        if vals.len() != s.fty.params.len() {
            crate::internal_error!(
                "indirect call arity {} != signature arity {}",
                vals.len(),
                s.fty.params.len()
            );
        }
        let t = self.operand(target);
        let fp = self.ft_entry(&t);
        let coerced = self.coerce_args(&s, &vals);
        self.call_ptr(&fp, &s, coerced).map(V::s)
    }

    pub fn iface_expr(
        &mut self,
        receiver: &Operand,
        iface_id: usize,
        method_slot: usize,
        sig: TypeId,
        args: &[Operand],
    ) -> Option<V> {
        let recv = self.operand(receiver);
        let mut call_args = vec![recv.clone()];
        for a in args {
            self.retain_rc_global_sink(true, a);
            call_args.push(self.operand(a));
        }
        let (td, _, _) = fn_sig(self.interner, sig);
        let s = fn_ptr_sig(self.interner, sig, &self.h(), &self.word());
        let dispatch = c_ident(&format!("__iface_dispatch_{td}"));
        let itable = V::s(Value::global(format!(
            "dream_iface_{iface_id}_{method_slot}"
        )));
        let mut arms: Vec<(i32, String)> = Vec::new();
        for (tag, cname) in self
            .l
            .cx
            .iface_guard(iface_id, method_slot)
            .unwrap_or_default()
        {
            if !arms.iter().any(|(t, _)| t == tag) {
                arms.push((*tag, cname.clone()));
            }
        }
        let ret_ty = s.fty.ret.clone();
        let mut dispatch_args = vec![itable];
        dispatch_args.extend(call_args.iter().cloned());
        if arms.is_empty() {
            return self.call(&dispatch, &dispatch_args);
        }
        let result = (!ret_ty.is_void()).then(|| self.w.alloca(ret_ty.clone(), 8));
        let tag = self.call_v("dream_object_tag", &[recv]);
        let tag = self.conv(&tag, &Ty::I32);
        let join = self.w.new_block("iface.join");
        let fallback = self.w.new_block("iface.itable");
        let blocks: Vec<_> = arms
            .iter()
            .map(|(t, _)| (*t as i128, self.w.new_block("iface.direct")))
            .collect();
        self.w.switch(&tag, fallback, &blocks);
        for ((_, b), (_, cname)) in blocks.iter().zip(&arms) {
            self.w.switch_to(*b);
            let callee = self.l.fn_ref(&self.l.abi_sym(cname));
            let vals = self.coerce_args(&s, &call_args);
            let r = self.call_ptr(&callee, &s, vals);
            if let (Some(slot), Some(r)) = (&result, r) {
                self.w.store(&r, slot, 8, &[]);
            }
            self.w.br(join);
        }
        self.w.switch_to(fallback);
        let r = self.call(&dispatch, &dispatch_args);
        if let (Some(slot), Some(r)) = (&result, r) {
            let r = self.conv(&r, &ret_ty);
            self.w.store(&r, slot, 8, &[]);
        }
        self.w.br(join);
        self.w.switch_to(join);
        result.map(|slot| V::s(self.w.load(ret_ty, &slot, 8, &[])))
    }
}
