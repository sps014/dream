//! Statements: assignments (with the in-place construction and `_into` reuse fusions),
//! ownership ops, prints, runtime hooks and the SIMD helpers.

use super::fx::{Fx, V};
use super::ir::{Ty, Value};
use crate::backend::shared::abi_types::{elem_size, native_scalar_size, runtime_c_name};
use crate::backend::shared::glue::{destroy_sym, release_into_sym, release_sym, retain_sym};
use crate::backend::shared::place_policy::{
    has_frame_buffer, is_alias_value_local, is_value_place_alias,
};
use crate::backend::shared::protocol_names::to_string_fn;
use crate::{Callee, Local, Operand, Place, Rvalue, Statement};
use dream_abi::intrinsics::IntrinsicOp;
use dream_types::{PrimTy, TyKind, TypeId};

impl<'l, 'a> Fx<'l, 'a> {
    pub fn stmts(&mut self, stmts: &[Statement]) {
        let mut i = 0;
        while i < stmts.len() {
            if let Some(skip) = self.try_emit_into(stmts, i) {
                i += skip;
                continue;
            }
            self.stmt(&stmts[i]);
            i += 1;
        }
    }

    fn is_substring_call(&self, callee: &Callee) -> bool {
        self.mir.intrinsics.iter().any(|(def, key)| {
            *def == callee.def && IntrinsicOp::from_key(key) == Some(IntrinsicOp::StringSubstring)
        })
    }

    fn is_into_rvalue(&self, rv: &Rvalue) -> bool {
        match rv {
            Rvalue::Concat(parts) => parts.len() == 2,
            Rvalue::ConcatInt { .. } => true,
            Rvalue::Call { callee, .. } => self.is_substring_call(callee),
            _ => false,
        }
    }

    /// `dest = rv` where `dest`'s old string is released first: the `_into` runtime entry reuses
    /// the old block in place when it is uniquely owned.
    fn emit_into(&mut self, dest: Local, rv: &Rvalue) {
        let slot = self.read_local(dest);
        let r = match rv {
            Rvalue::Concat(parts) if parts.len() == 2 => {
                let (a, b) = (self.operand(&parts[0]), self.operand(&parts[1]));
                self.call_v("dream_concat_strings_into", &[slot, a, b])
            }
            Rvalue::ConcatInt {
                prefix,
                value,
                suffix,
            } => {
                let p = self.operand(prefix);
                let v = self.operand(value);
                let v = self.conv_v(&v, &Ty::I32, false);
                let s = self.operand(suffix);
                self.call_v("dream_concat_str_int_str_into", &[slot, p, v, s])
            }
            Rvalue::Call { args, .. } => {
                let mut all = vec![slot];
                all.extend(args.iter().map(|a| self.operand(a)));
                self.call_v("dream_substring_into", &all)
            }
            _ => crate::internal_error!("into emit of non-reusable rvalue"),
        };
        self.store(&Place::Local(dest), rv, r);
    }

    fn try_emit_into(&mut self, stmts: &[Statement], i: usize) -> Option<usize> {
        if i + 1 < stmts.len() {
            if let (
                Statement::Release(Operand::Copy(Place::Local(rel)))
                | Statement::ReleaseUnique(Operand::Copy(Place::Local(rel))),
                Statement::Assign(Place::Local(dest), rv),
            ) = (&stmts[i], &stmts[i + 1])
            {
                if rel.0 == dest.0
                    && self.is_into_rvalue(rv)
                    && !crate::passes::rvalue_reads_local(rv, dest.0)
                {
                    self.emit_into(*dest, rv);
                    return Some(2);
                }
            }
        }
        if i + 2 < stmts.len() {
            if let (
                Statement::Assign(Place::Local(tmp), rv),
                Statement::Release(Operand::Copy(Place::Local(rel)))
                | Statement::ReleaseUnique(Operand::Copy(Place::Local(rel))),
                Statement::Assign(
                    Place::Local(dest),
                    Rvalue::Use(Operand::Copy(Place::Local(src))),
                ),
            ) = (&stmts[i], &stmts[i + 1], &stmts[i + 2])
            {
                if src.0 == tmp.0
                    && rel.0 == dest.0
                    && tmp.0 != dest.0
                    && self.is_into_rvalue(rv)
                    && !crate::passes::rvalue_reads_local(rv, dest.0)
                {
                    self.emit_into(*dest, rv);
                    let v = self.read_local(*dest);
                    self.write_local(*tmp, &v);
                    return Some(3);
                }
            }
        }
        None
    }

    pub fn stmt(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Nop | Statement::SourceLine(_) => {}
            Statement::DebugLine(line) => self.set_line(*line),
            Statement::Assign(place, rv) => self.assign(place, rv),
            Statement::Retain(o) => {
                let ty = self.operand_ty(o);
                let a = self.operand(o);
                let sym = retain_sym(&self.l.cx, ty);
                self.call(sym, &[a]);
            }
            Statement::Release(o) => {
                let ty = self.operand_ty(o);
                let a = self.operand(o);
                // Inline fast path: null check + decrement here; the free tail runs only on the
                // last-ref transition.
                if let Some(tail) = release_into_sym(&self.l.cx, ty) {
                    let a = self.as_ref(&a);
                    let nz = self.truthy(&a);
                    self.if_then(&nz, |fx| {
                        let last = fx.call_v("dream_rc_last", std::slice::from_ref(&a));
                        let last = fx.truthy(&last);
                        fx.if_then(&last, |fx| {
                            fx.call(&tail, std::slice::from_ref(&a));
                        });
                    });
                } else {
                    let sym = if self.is_rc(ty) {
                        release_sym(&self.l.cx, ty)
                    } else {
                        "dream_release".into()
                    };
                    self.call(&sym, &[a]);
                }
            }
            Statement::ReleaseUnique(o) => {
                let ty = self.operand_ty(o);
                let sym = if self.is_rc(ty) {
                    destroy_sym(&self.l.cx, ty)
                } else {
                    "dream_destroy".into()
                };
                let a = self.operand(o);
                self.call(&sym, &[a]);
            }
            Statement::Panic(o) => {
                let a = self.operand(o);
                self.call("dream_panic", &[a]);
            }
            Statement::Print { arg, ty, newline } => self.print(arg, *ty, *newline),
            Statement::Call { callee, args } => {
                if !self.simd_call(callee, args) {
                    self.call_expr(callee, args);
                }
            }
            Statement::JsCall {
                callee,
                target,
                via,
                method,
                args,
            } => {
                self.js_call(callee, target, via, method, args);
            }
            Statement::InterfaceCall {
                receiver,
                iface_id,
                method_slot,
                sig,
                args,
            } => {
                self.iface_expr(receiver, *iface_id, *method_slot, *sig, args);
            }
            Statement::IndirectCall { target, args, sig } => {
                self.indirect_expr(target, args, *sig);
            }
            Statement::ArrayElemsCopy {
                dst,
                dst_off,
                src,
                src_off,
                count,
                elem_ty,
            } => {
                let es = elem_size(&self.l.cx, *elem_ty) as i64;
                let d = self.operand(dst);
                let doff = self.operand(dst_off);
                let s = self.operand(src);
                let soff = self.operand(src_off);
                let n = self.operand(count);
                let da = self.elem_addr(&d, &doff, es);
                let sa = self.elem_addr(&s, &soff, es);
                let bytes = self.scaled(&n, es);
                self.call("dream_mem_copy", &[V::s(da), V::s(sa), V::u(bytes)]);
            }
            Statement::ArrayElemsFill {
                dst,
                dst_off,
                count,
                elem_ty,
            } => {
                let es = elem_size(&self.l.cx, *elem_ty) as i64;
                let d = self.operand(dst);
                let doff = self.operand(dst_off);
                let n = self.operand(count);
                let da = self.elem_addr(&d, &doff, es);
                let bytes = self.scaled(&n, es);
                self.memset0(&da, &bytes);
            }
            Statement::ForceFree(o) => {
                let a = self.operand(o);
                self.call("dream_free", &[a]);
            }
            Statement::LockAcquire(o) => {
                let a = self.lock_addr(o);
                self.call("dream_lock_acquire", &[a]);
            }
            Statement::LockRelease(o) => {
                let a = self.lock_addr(o);
                self.call("dream_lock_release", &[a]);
            }
            Statement::DeferEnter => {
                self.call("dream_defer_enter", &[]);
            }
            Statement::RegionEnter => {
                self.call("dream_region_enter", &[]);
            }
            Statement::RegionLeave => {
                self.call("dream_region_leave", &[]);
            }
            Statement::DeferLeave(o) => {
                let a = self.operand(o);
                let a = self.conv_v(&a, &Ty::I32, true);
                self.call("dream_defer_leave", &[a]);
            }
            Statement::SimdV128 {
                dest,
                lhs,
                rhs,
                index,
                splat_rhs,
                ptr_addr,
                op,
                lane,
            } => {
                let d = self.operand(dest);
                let l = self.operand(lhs);
                let r = self.operand(rhs);
                let i = self.operand(index);
                let es = lane.esize() as i64;
                let opi = match op {
                    crate::BinOp::Sub => 1,
                    crate::BinOp::Mul => 2,
                    crate::BinOp::Div => 3,
                    _ => 0,
                };
                let raddr = match splat_rhs {
                    Some(s) => self.operand(s),
                    None => r,
                };
                let (d, l, raddr) = if *ptr_addr {
                    (d, l, raddr)
                } else {
                    let (a, b, c) = (
                        self.elem_addr(&d, &i, es),
                        self.elem_addr(&l, &i, es),
                        self.elem_addr(&raddr, &i, es),
                    );
                    (V::s(a), V::s(b), V::s(c))
                };
                self.call("dream_simd_binop", &[d, l, raddr, V::i32(es), V::i32(opi)]);
            }
            Statement::ValueDrop(l) => {
                if !self.f.locals[l.0 as usize].is_ref && !is_alias_value_local(self.f, *l) {
                    let v = self.read_local(*l);
                    self.value_refs(self.f.local_ty(*l), &v, false);
                }
            }
            Statement::ValueRetain(l) => {
                if !is_alias_value_local(self.f, *l) {
                    let v = self.read_local(*l);
                    self.value_refs(self.f.local_ty(*l), &v, true);
                }
            }
            Statement::ValueKill(l) => {
                let size = elem_size(&self.l.cx, self.f.local_ty(*l)) as i64;
                let v = self.read_local(*l);
                let p = self.ptr(&v);
                self.memset0(&p, &Value::i64(size));
            }
        }
    }

    fn assign(&mut self, place: &Place, rv: &Rvalue) {
        if self.simd_assign(place, rv) {
            return;
        }
        if let (
            Place::Local(_),
            Rvalue::ArrayNew {
                elem_ty,
                len,
                closure_env: true,
            },
        ) = (place, rv)
        {
            let es = elem_size(&self.l.cx, *elem_ty);
            let n = self.operand(len);
            let r = self.call_v("dream_closure_env_array_new", &[n, V::i32(es as i64)]);
            self.store(place, rv, r);
            return;
        }
        if let (
            Place::Local(l),
            Rvalue::UnionNew {
                ty, variant, args, ..
            },
        ) = (place, rv)
        {
            if self.is_value(self.f.local_ty(*l)) && !is_value_place_alias(self.f, *l, rv) {
                let dest = self.read_local(*l);
                self.union_new_at(&dest, *ty, *variant, args);
                return;
            }
        }
        if let (Place::Local(l), Rvalue::New { ty, ctor, args, .. }) = (place, rv) {
            if has_frame_buffer(self.mir, self.f, *l) {
                let buf = self.frame_buf(*l);
                let o = self.emit_new_in(*ty, ctor.as_ref().map(|c| c.def), args, Some(buf));
                self.write_local(*l, &o);
                return;
            }
            if self.is_value(self.f.local_ty(*l)) && !is_value_place_alias(self.f, *l, rv) {
                let dest = self.read_local(*l);
                self.struct_new_at(&dest, *ty, ctor.as_ref().map(|c| c.def), args);
                return;
            }
        }
        if self.store_from_bytes_value(place, rv) || self.store_js_to_value(place, rv) {
            return;
        }
        let dest = self.operand_ty(&Operand::Copy(place.clone()));
        let r = self.rvalue(rv, Some(dest));
        self.store(place, rv, r);
    }

    /// `(char *)dream_p(base) + 4 + off * es`.
    fn elem_addr(&mut self, base: &V, off: &V, es: i64) -> Value {
        let o = self.conv(off, &Ty::I64);
        let scaled = self.w.bin("mul", &o, &Value::i64(es));
        let data = self.addr(base, crate::abi::LEN_PREFIX_SIZE as i64);
        self.w.gep_i8(&data, &scaled)
    }

    fn scaled(&mut self, n: &V, es: i64) -> Value {
        let n = self.conv(n, &Ty::I64);
        self.w.bin("mul", &n, &Value::i64(es))
    }

    fn lock_addr(&mut self, o: &Operand) -> V {
        let ty = self.operand_ty(o);
        let base = self.operand(o);
        let size = self.l.cx.nstruct(ty).map(|l| l.size).unwrap_or(0);
        if size == 0 {
            return base;
        }
        let b = self.as_ref(&base);
        V::u(self.w.bin("add", &b.v, &Value::i64(size as i64)))
    }

    pub(super) fn value_dest(&mut self, place: &Place) -> Option<V> {
        match place {
            Place::Local(l) if self.is_value(self.f.local_ty(*l)) => Some(self.read_local(*l)),
            Place::Field { base, field } => {
                let layout = self.l.cx.nstruct(self.f.local_ty(*base))?;
                let fld = layout.fields.get(*field)?.clone();
                if !self.is_value(fld.ty) {
                    return None;
                }
                let b = self.read_local(*base);
                let at = self.addr(&b, fld.offset as i64);
                Some(self.as_ref(&V::s(at)))
            }
            _ => None,
        }
    }

    /// `dream_from_bytes` hands back a heap box; a value-struct destination copies it out and
    /// frees it here (its type is not RC-tracked, so no scope-exit release covers it).
    fn store_from_bytes_value(&mut self, place: &Place, rv: &Rvalue) -> bool {
        let Rvalue::FromBytes { ty, .. } = rv else {
            return false;
        };
        if !self.is_value(*ty) {
            return false;
        }
        let Some(dst) = self.value_dest(place) else {
            return false;
        };
        let size = native_scalar_size(&self.l.cx, *ty).0.max(1) as i64;
        let boxed = self.rvalue(rv, None);
        let (dp, bp) = (self.ptr(&dst), self.ptr(&boxed));
        self.memcpy(&dp, &bp, &Value::i64(size));
        self.call("dream_release", &[boxed]);
        true
    }

    fn simd_call_name(&self, callee: &Callee) -> String {
        let raw = self.l.cx.callee_sym(callee.def, &callee.args);
        let mapped = runtime_c_name(&raw);
        if mapped.starts_with("simd_") {
            return mapped;
        }
        match IntrinsicOp::from_key(&raw).or_else(|| IntrinsicOp::from_key(&mapped)) {
            Some(IntrinsicOp::SimdLaneCount) => "simd_lane_count".into(),
            Some(IntrinsicOp::SimdV128Load) => "simd_v128_load".into(),
            Some(IntrinsicOp::SimdV128Store) => "simd_v128_store".into(),
            Some(IntrinsicOp::SimdV128Splat) => "simd_v128_splat".into(),
            Some(IntrinsicOp::SimdV128Add) => "simd_v128_add".into(),
            Some(IntrinsicOp::SimdV128Sub) => "simd_v128_sub".into(),
            Some(IntrinsicOp::SimdV128Mul) => "simd_v128_mul".into(),
            Some(IntrinsicOp::SimdV128Min) => "simd_v128_min".into(),
            Some(IntrinsicOp::SimdV128Max) => "simd_v128_max".into(),
            Some(IntrinsicOp::SimdV128Sum) => "simd_v128_sum".into(),
            _ => mapped,
        }
    }

    fn simd_es(&self, callee: &Callee) -> i64 {
        callee
            .args
            .first()
            .map(|ty| elem_size(&self.l.cx, *ty))
            .unwrap_or(4)
            .max(1) as i64
    }

    fn simd_assign(&mut self, place: &Place, rv: &Rvalue) -> bool {
        let Rvalue::Call { callee, args } = rv else {
            return false;
        };
        let name = self.simd_call_name(callee);
        if name == "simd_lane_count" {
            let Place::Local(l) = place else {
                return false;
            };
            self.write_local(*l, &V::i32(4));
            return true;
        }
        if name == "simd_v128_sum" {
            let Place::Local(l) = place else {
                return false;
            };
            let a = self.operand(&args[0]);
            let s = self.call_v("simd_v128_sum", &[a]);
            self.write_local(*l, &s);
            return true;
        }
        let op = match name.as_str() {
            "simd_v128_load" if args.len() >= 2 => None,
            "simd_v128_splat" if args.len() == 1 => None,
            "simd_v128_add" if args.len() >= 2 => Some(0),
            "simd_v128_sub" if args.len() >= 2 => Some(1),
            "simd_v128_mul" if args.len() >= 2 => Some(2),
            "simd_v128_min" if args.len() >= 2 => Some(3),
            "simd_v128_max" if args.len() >= 2 => Some(4),
            _ => return false,
        };
        let Some(dest) = self.value_dest(place) else {
            return false;
        };
        let es = self.simd_es(callee);
        let a: Vec<V> = args.iter().map(|o| self.operand(o)).collect();
        let dp = self.ptr(&dest);
        match (name.as_str(), op) {
            ("simd_v128_load", _) => {
                let i = self.conv_v(&a[1], &Ty::I64, true);
                let src = self.elem_addr(&a[0], &i, es);
                self.memcpy(&dp, &src, &Value::i64(16));
            }
            ("simd_v128_splat", _) => {
                let x = self.conv_v(&a[0], &Ty::F32, false);
                self.call("dream_v128_splat_f32", &[V::s(dp), x]);
            }
            (_, Some(k)) => {
                let (x, y) = (self.ptr(&a[0]), self.ptr(&a[1]));
                self.call(
                    "dream_v128_f32_bin",
                    &[V::s(dp), V::s(x), V::s(y), V::i32(k)],
                );
            }
            _ => return false,
        }
        true
    }

    fn simd_call(&mut self, callee: &Callee, args: &[Operand]) -> bool {
        let name = self.simd_call_name(callee);
        if name != "simd_v128_store" || args.len() < 3 {
            return false;
        }
        let es = self.simd_es(callee);
        let a1 = self.operand(&args[1]);
        let a2 = self.operand(&args[2]);
        let a0 = self.operand(&args[0]);
        let i = self.conv_v(&a2, &Ty::I64, true);
        let dst = self.elem_addr(&a1, &i, es);
        let src = self.ptr(&a0);
        self.memcpy(&dst, &src, &Value::i64(16));
        true
    }

    fn print(&mut self, arg: &Operand, ty: TypeId, newline: bool) {
        let a = self.operand(arg);
        match self.interner.kind(ty) {
            TyKind::Prim(PrimTy::Int) | TyKind::Enum(_) => {
                let x = self.conv_v(&a, &Ty::I32, false);
                self.call("print_int", &[x]);
            }
            TyKind::Prim(PrimTy::Char) => {
                let x = self.conv_v(&a, &Ty::I32, false);
                self.call("print_char", &[x]);
            }
            TyKind::Prim(PrimTy::String) => {
                self.call("print_string", &[a]);
            }
            TyKind::Prim(PrimTy::Float) => {
                let x = self.conv_v(&a, &Ty::F32, false);
                self.call("print_float", &[x]);
            }
            TyKind::Prim(PrimTy::Double) => {
                let x = self.conv_v(&a, &Ty::F64, false);
                self.call("print_double", &[x]);
            }
            _ => {
                let conv = to_string_fn(&self.l.cx, ty);
                if conv.is_empty() {
                    self.call("print_string", &[a]);
                } else {
                    let s = self.call_v(&conv, &[a]);
                    self.call("print_string", std::slice::from_ref(&s));
                    self.call("dream_release", &[s]);
                }
            }
        }
        if newline {
            self.call("print_char", &[V::i32(10)]);
        }
    }
}
