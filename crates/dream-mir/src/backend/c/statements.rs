use super::ast::{CTy, CaseKey, Expr, Stmt, SwitchArm};
use super::emit::Emitter;
use super::places::{is_alias_value_local, is_value_place_alias};
use super::types::{elem_size, runtime_c_name};
use crate::{Callee, Local, Operand, Place, Rvalue, Statement};
use dream_abi::intrinsics::IntrinsicOp;
use dream_types::{PrimTy, TyKind};

impl<'a> Emitter<'a> {
    pub(super) fn stmts(&mut self, stmts: &[Statement]) {
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
        self.cx.mir.intrinsics.iter().any(|(def, key)| {
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

    fn emit_into(&mut self, dest: Local, rv: &Rvalue) {
        let slot = Expr::local(dest.0);
        let rhs = match rv {
            Rvalue::Concat(parts) if parts.len() == 2 => Expr::call(
                "dream_concat_strings_into",
                vec![
                    slot.clone(),
                    self.operand(&parts[0]),
                    self.operand(&parts[1]),
                ],
            ),
            Rvalue::ConcatInt {
                prefix,
                value,
                suffix,
            } => Expr::call(
                "dream_concat_str_int_str_into",
                vec![
                    slot.clone(),
                    self.operand(prefix),
                    Expr::cast(CTy::I32, self.operand(value)),
                    self.operand(suffix),
                ],
            ),
            Rvalue::Call { args, .. } => {
                let mut call_args = vec![slot.clone()];
                call_args.extend(args.iter().map(|a| self.operand(a)));
                Expr::call("dream_substring_into", call_args)
            }
            _ => crate::internal_error!("into emit of non-reusable rvalue"),
        };
        let stored = self.store(&Place::Local(dest), rv, rhs);
        self.b.expr_stmt(stored);
    }

    fn try_emit_into(&mut self, stmts: &[Statement], i: usize) -> Option<usize> {
        if i + 1 < stmts.len() {
            if let (
                Statement::Release(Operand::Copy(Place::Local(rel))),
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
                Statement::Release(Operand::Copy(Place::Local(rel))),
                Statement::Assign(Place::Local(dest), Rvalue::Use(Operand::Copy(Place::Local(src)))),
            ) = (&stmts[i], &stmts[i + 1], &stmts[i + 2])
            {
                if src.0 == tmp.0
                    && rel.0 == dest.0
                    && tmp.0 != dest.0
                    && self.is_into_rvalue(rv)
                    && !crate::passes::rvalue_reads_local(rv, dest.0)
                {
                    self.emit_into(*dest, rv);
                    self.b.assign(Expr::local(tmp.0), Expr::local(dest.0));
                    return Some(3);
                }
            }
        }
        None
    }

    pub(super) fn stmt(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Nop | Statement::SourceLine(_) => {}
            Statement::DebugLine(line) => {
                self.b.stmt(Stmt::Line {
                    file: self.f.file.clone().unwrap_or_default(),
                    line: *line,
                });
            }
            Statement::Assign(place, rv) => {
                if self.simd_assign(place, rv) {
                    return;
                }
                if let (
                    Place::Local(l),
                    crate::Rvalue::UnionNew {
                        ty, variant, args, ..
                    },
                ) = (place, rv)
                {
                    if self.cx.interner.is_value_type(self.f.local_ty(*l))
                        && !is_value_place_alias(self.f, *l, rv)
                    {
                        self.union_new_at(Expr::local(l.0), *ty, *variant, args);
                        return;
                    }
                }
                let rhs = self.rvalue(rv);
                let stored = self.store(place, rv, rhs);
                self.b.expr_stmt(stored);
            }
            Statement::Retain(o) => {
                let a = self.operand(o);
                self.b.call("dream_retain", vec![a]);
            }
            Statement::Release(o) => {
                let ty = self.operand_ty(o);
                let release = if self.cx.interner.is_rc_tracked(ty) {
                    crate::backend::c::release::release_sym(self.cx.interner, self.cx.mir, ty)
                } else {
                    "dream_release".into()
                };
                let a = self.operand(o);
                self.b.call(release, vec![a]);
            }
            Statement::Panic(o) => {
                let a = self.operand(o);
                self.b.call("dream_panic", vec![a]);
            }
            Statement::Print { arg, ty, newline } => self.print(arg, *ty, *newline),
            Statement::Call { callee, args } => {
                if self.simd_call(callee, args) {
                    return;
                }
                let e = self.call_expr(callee, args);
                self.b.expr_stmt(e);
            }
            Statement::JsCall {
                target,
                via,
                method,
                args,
                ..
            } => {
                let e = self.js_call_expr(target, via, method, args);
                self.b.expr_stmt(Expr::cast(CTy::Void, e));
            }
            Statement::InterfaceCall {
                receiver,
                iface_id,
                method_slot,
                args,
                ..
            } => {
                let e = self.iface_expr(receiver, *iface_id, *method_slot, args);
                self.b.expr_stmt(Expr::cast(CTy::Void, e));
            }
            Statement::IndirectCall { target, args, sig } => {
                let e = self.indirect_expr(target, args, *sig);
                self.b.expr_stmt(Expr::cast(CTy::Void, e));
            }
            Statement::ArrayElemsCopy {
                dst,
                dst_off,
                src,
                src_off,
                count,
                elem_ty,
            } => {
                let es = elem_size(self.cx, *elem_ty);
                let d = self.operand(dst);
                let doff = self.operand(dst_off);
                let s = self.operand(src);
                let soff = self.operand(src_off);
                let n = self.operand(count);
                self.b.call(
                    "dream_mem_copy",
                    vec![
                        Expr::add(
                            d,
                            Expr::add(
                                super::types::len_prefix(),
                                Expr::mul(doff, Expr::i(es as i64)),
                            ),
                        ),
                        Expr::add(
                            s,
                            Expr::add(
                                super::types::len_prefix(),
                                Expr::mul(soff, Expr::i(es as i64)),
                            ),
                        ),
                        Expr::cast(CTy::Named("size_t"), Expr::mul(n, Expr::i(es as i64))),
                    ],
                );
            }
            Statement::ArrayElemsFill {
                dst,
                dst_off,
                count,
                elem_ty,
            } => {
                let es = elem_size(self.cx, *elem_ty);
                let d = self.operand(dst);
                let doff = self.operand(dst_off);
                let n = self.operand(count);
                self.b.call(
                    "memset",
                    vec![
                        Expr::add(
                            Expr::ptr_add(d, super::types::len_prefix()),
                            Expr::mul(doff, Expr::i(es as i64)),
                        ),
                        Expr::i(0),
                        Expr::cast(CTy::Named("size_t"), Expr::mul(n, Expr::i(es as i64))),
                    ],
                );
            }
            Statement::ForceFree(o) => {
                let a = self.operand(o);
                self.b.call("dream_free", vec![a]);
            }
            Statement::LockAcquire(o) => {
                let a = self.operand(o);
                self.b.call("dream_lock_acquire", vec![a]);
            }
            Statement::LockRelease(o) => {
                let a = self.operand(o);
                self.b.call("dream_lock_release", vec![a]);
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
                let es = lane.esize();
                let opi = match op {
                    crate::BinOp::Add => 0,
                    crate::BinOp::Sub => 1,
                    crate::BinOp::Mul => 2,
                    crate::BinOp::Div => 3,
                    _ => 0,
                };
                let raddr = splat_rhs.as_ref().map(|s| self.operand(s)).unwrap_or(r);
                let call = if *ptr_addr {
                    Expr::call(
                        "dream_simd_binop",
                        vec![d, l, raddr, Expr::i(es as i64), Expr::i(opi)],
                    )
                } else {
                    let off = |base: Expr| {
                        Expr::add(
                            base,
                            Expr::add(
                                super::types::len_prefix(),
                                Expr::mul(i.clone(), Expr::i(es as i64)),
                            ),
                        )
                    };
                    Expr::call(
                        "dream_simd_binop",
                        vec![off(d), off(l), off(raddr), Expr::i(es as i64), Expr::i(opi)],
                    )
                };
                self.b.expr_stmt(call);
            }
            Statement::ValueDrop(l) => {
                if !self.f.locals[l.0 as usize].is_ref && !is_alias_value_local(self.f, *l) {
                    self.value_refs(self.f.local_ty(*l), Expr::local(l.0), false);
                }
            }
            Statement::ValueRetain(l) => {
                if !is_alias_value_local(self.f, *l) {
                    self.value_refs(self.f.local_ty(*l), Expr::local(l.0), true);
                }
            }
            Statement::ValueKill(l) => {
                let size = elem_size(self.cx, self.f.local_ty(*l));
                self.b.call(
                    "memset",
                    vec![
                        Expr::dream_p(Expr::local(l.0)),
                        Expr::i(0),
                        Expr::i(size as i64),
                    ],
                );
            }
        }
    }

    pub(super) fn value_refs(&mut self, ty: dream_types::TypeId, base: Expr, retain: bool) {
        for s in value_ref_stmts(self.cx, ty, base, retain) {
            self.b.stmt(s);
        }
    }
}

pub(super) fn value_ref_stmts(
    cx: &super::ctx::Cx<'_>,
    ty: dream_types::TypeId,
    base: Expr,
    retain: bool,
) -> Vec<Stmt> {
    let mut out = Vec::new();
    if let Some(layout) = cx.nstruct(ty) {
        for field in &layout.fields {
            if field.is_weak || field.is_unowned {
                continue;
            }
            let at = Expr::cast(
                CTy::Ptr,
                Expr::ptr_add(base.clone(), Expr::i(field.offset as i64)),
            );
            if cx.interner.is_value_type(field.ty) {
                out.extend(value_ref_stmts(cx, field.ty, at, retain));
            } else if cx.interner.is_rc_tracked(field.ty) {
                let value = Expr::load(CTy::Ptr, Expr::dream_p(at));
                if retain {
                    out.push(Stmt::call("dream_retain", vec![value]));
                } else {
                    let release =
                        crate::backend::c::release::release_sym(cx.interner, cx.mir, field.ty);
                    out.push(Stmt::call(release, vec![value]));
                }
            }
        }
        return out;
    }
    let Some(u) = cx.nunion(ty) else {
        return out;
    };
    let mut arms = Vec::new();
    for variant in &u.variants {
        let mut body = Vec::new();
        for field in &variant.fields {
            if field.is_weak || field.is_unowned {
                continue;
            }
            let at = Expr::cast(
                CTy::Ptr,
                Expr::ptr_add(base.clone(), Expr::i(field.offset as i64)),
            );
            if cx.interner.is_value_type(field.ty) {
                body.extend(value_ref_stmts(cx, field.ty, at, retain));
            } else if cx.interner.is_rc_tracked(field.ty) {
                let value = Expr::load(CTy::Ptr, Expr::dream_p(at));
                if retain {
                    body.push(Stmt::call("dream_retain", vec![value]));
                } else {
                    let release =
                        crate::backend::c::release::release_sym(cx.interner, cx.mir, field.ty);
                    body.push(Stmt::call(release, vec![value]));
                }
            }
        }
        body.push(Stmt::Expr(Expr::id("break")));
        arms.push(SwitchArm {
            keys: vec![CaseKey::Int(variant.discriminant as i64)],
            body,
        });
    }
    arms.push(SwitchArm {
        keys: vec![],
        body: vec![Stmt::Expr(Expr::id("break"))],
    });
    out.push(Stmt::Switch {
        expr: Expr::load(CTy::I32, Expr::dream_p(base)),
        arms,
    });
    out
}

impl<'a> Emitter<'a> {
    fn simd_call_name(&self, callee: &Callee) -> String {
        let raw = self.cx.callee_c(callee.def, &callee.args);
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

    fn simd_es(&self, callee: &Callee) -> u32 {
        callee
            .args
            .first()
            .map(|ty| elem_size(self.cx, *ty))
            .unwrap_or(4)
            .max(1)
    }

    fn value_dest(&self, place: &Place) -> Option<Expr> {
        match place {
            Place::Local(l) if self.cx.interner.is_value_type(self.f.local_ty(*l)) => {
                Some(Expr::dream_p(Expr::local(l.0)))
            }
            Place::Field { base, field } => {
                let layout = self.cx.nstruct(self.f.local_ty(*base))?;
                let fld = layout.fields.get(*field)?;
                if self.cx.interner.is_value_type(fld.ty) {
                    Some(Expr::field_ptr(base.0, fld.offset))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn simd_assign(&mut self, place: &Place, rv: &crate::Rvalue) -> bool {
        let crate::Rvalue::Call { callee, args } = rv else {
            return false;
        };
        let name = self.simd_call_name(callee);
        if name == "simd_lane_count" {
            let Place::Local(l) = place else {
                return false;
            };
            self.b.assign(Expr::local(l.0), Expr::i(4));
            return true;
        }
        if name == "simd_v128_sum" {
            let Place::Local(l) = place else {
                return false;
            };
            let a = self.operand(&args[0]);
            self.b
                .assign(Expr::local(l.0), Expr::call("simd_v128_sum", vec![a]));
            return true;
        }
        let Some(dest) = self.value_dest(place) else {
            return false;
        };
        let es = self.simd_es(callee);
        let a: Vec<Expr> = args.iter().map(|o| self.operand(o)).collect();
        match name.as_str() {
            "simd_v128_load" if a.len() >= 2 => {
                self.b.call(
                    "memcpy",
                    vec![
                        dest,
                        Expr::add(
                            Expr::ptr_add(a[0].clone(), super::types::len_prefix()),
                            Expr::mul(
                                Expr::cast(CTy::Named("size_t"), a[1].clone()),
                                Expr::i(es as i64),
                            ),
                        ),
                        Expr::i(16),
                    ],
                );
                true
            }
            "simd_v128_splat" if a.len() == 1 => {
                self.b.call(
                    "dream_v128_splat_f32",
                    vec![dest, Expr::cast(CTy::F32, a[0].clone())],
                );
                true
            }
            "simd_v128_add" if a.len() >= 2 => {
                self.b.call(
                    "dream_v128_f32_bin",
                    vec![
                        dest,
                        Expr::dream_p(a[0].clone()),
                        Expr::dream_p(a[1].clone()),
                        Expr::i(0),
                    ],
                );
                true
            }
            "simd_v128_sub" if a.len() >= 2 => {
                self.b.call(
                    "dream_v128_f32_bin",
                    vec![
                        dest,
                        Expr::dream_p(a[0].clone()),
                        Expr::dream_p(a[1].clone()),
                        Expr::i(1),
                    ],
                );
                true
            }
            "simd_v128_mul" if a.len() >= 2 => {
                self.b.call(
                    "dream_v128_f32_bin",
                    vec![
                        dest,
                        Expr::dream_p(a[0].clone()),
                        Expr::dream_p(a[1].clone()),
                        Expr::i(2),
                    ],
                );
                true
            }
            "simd_v128_min" if a.len() >= 2 => {
                self.b.call(
                    "dream_v128_f32_bin",
                    vec![
                        dest,
                        Expr::dream_p(a[0].clone()),
                        Expr::dream_p(a[1].clone()),
                        Expr::i(3),
                    ],
                );
                true
            }
            "simd_v128_max" if a.len() >= 2 => {
                self.b.call(
                    "dream_v128_f32_bin",
                    vec![
                        dest,
                        Expr::dream_p(a[0].clone()),
                        Expr::dream_p(a[1].clone()),
                        Expr::i(4),
                    ],
                );
                true
            }
            _ => false,
        }
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
        self.b.call(
            "memcpy",
            vec![
                Expr::add(
                    Expr::ptr_add(a1, super::types::len_prefix()),
                    Expr::mul(Expr::cast(CTy::Named("size_t"), a2), Expr::i(es as i64)),
                ),
                Expr::dream_p(a0),
                Expr::i(16),
            ],
        );
        true
    }

    fn print(&mut self, arg: &crate::Operand, ty: dream_types::TypeId, newline: bool) {
        let a = self.operand(arg);
        match self.cx.interner.kind(ty) {
            TyKind::Prim(PrimTy::Int) | TyKind::Enum(_) => {
                self.b.call("print_int", vec![Expr::cast(CTy::I32, a)]);
            }
            TyKind::Prim(PrimTy::Char) => {
                self.b.call("print_char", vec![Expr::cast(CTy::I32, a)]);
            }
            TyKind::Prim(PrimTy::String) => {
                self.b.call("print_string", vec![a]);
            }
            TyKind::Prim(PrimTy::Float) => {
                self.b.call("print_float", vec![Expr::cast(CTy::F32, a)]);
            }
            TyKind::Prim(PrimTy::Double) => {
                self.b.call("print_double", vec![Expr::cast(CTy::F64, a)]);
            }
            _ => {
                let conv = self.to_string_fn(ty);
                if conv.is_empty() {
                    self.b.call("print_string", vec![a]);
                } else {
                    self.b.stmt(Stmt::block(vec![
                        Stmt::decl(CTy::Ptr, "__ps", Some(Expr::call(conv, vec![a]))),
                        Stmt::call("print_string", vec![Expr::id("__ps")]),
                        Stmt::call("dream_release", vec![Expr::id("__ps")]),
                    ]));
                }
            }
        }
        if newline {
            self.b.call("print_char", vec![Expr::i(10)]);
        }
    }
}
