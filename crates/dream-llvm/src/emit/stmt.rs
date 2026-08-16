//! Statements, prints, value drop, coercions, terminators.

use super::ModuleEmitter;
use super::names::*;
use dream_hir::scalar_size;
use dream_mir::{MirFunction, Operand, Place, Statement, Terminator};
use dream_types::{PrimTy, TyKind, TypeId};
use std::fmt::Write as _;

impl<'a> ModuleEmitter<'a> {
    pub(crate) fn emit_stmt(&mut self, func: &MirFunction, stmt: &Statement) {
        match stmt {
            Statement::Nop | Statement::SourceLine(_) => {}
            Statement::DebugLine(line) => {
                if self.opts.debug_info && self.opts.triple.is_wasm() {
                    self.emit_debug_line(func, *line);
                }
            }
            Statement::Retain(o) => {
                let v = self.operand(func, o);
                let ty = self.op_ty(func, o);
                if matches!(self.interner.kind(ty), TyKind::Js) {
                    let _ = writeln!(self.buf, "  call void @d_js_retain(i32 {})", v);
                } else {
                    let _ = writeln!(self.buf, "  call void @dream_retain(i32 {})", v);
                }
            }
            Statement::Release(o) => {
                let v = self.operand(func, o);
                let ty = self.op_ty(func, o);
                if matches!(self.interner.kind(ty), TyKind::Js) {
                    let _ = writeln!(self.buf, "  call void @d_js_release(i32 {})", v);
                } else {
                    let _ = writeln!(self.buf, "  call void @dream_release(i32 {})", v);
                }
            }
            Statement::Print { arg, ty, newline } => {
                self.emit_print(func, arg, *ty, *newline);
            }
            Statement::Panic(o) => {
                let v = self.operand(func, o);
                let _ = writeln!(self.buf, "  call void @dream_panic(i32 {})", v);
                self.buf.push_str("  unreachable\n");
            }
            Statement::Assign(place, rv) => {
                let val = self.rvalue(func, rv, self.pl_ty(func, place));
                self.store_place(func, place, &val, retain_on_store(func, rv));
                if matches!(place, Place::Field { .. } | Place::Index { .. } | Place::Deref { .. })
                {
                    if let Some(src) = take_move_src(func, rv) {
                        let ty = llvm_val_ty(self.interner, func.local_ty(src));
                        if ty != "void" {
                            let _ = writeln!(
                                self.buf,
                                "  store {} {}, {}* %l{}",
                                ty,
                                zero(ty),
                                ty,
                                src.0
                            );
                        }
                    }
                }
            }
            Statement::Call { callee, args } => {
                self.emit_call(func, callee, args, true);
            }
            Statement::IndirectCall { target, args, sig } => {
                let _ = self.emit_indirect(func, target, args, *sig, true);
            }
            Statement::InterfaceCall {
                receiver,
                iface_id,
                method_slot,
                args,
                ..
            } => {
                let _ = self.emit_iface(func, receiver, *iface_id, *method_slot, args, true);
            }
            Statement::JsCall {
                callee,
                target,
                via,
                method,
                args,
            } => {
                let mut vals = vec![self.operand(func, target)];
                if let Some(v) = via {
                    vals.push(self.operand(func, v));
                }
                if let Some(m) = method {
                    vals.push(self.operand(func, m));
                }
                for (a, _) in args {
                    vals.push(self.operand(func, a));
                }
                let _ = self.emit_js_vals(callee, vals, true);
            }
            Statement::ArrayElemsCopy {
                elem_ty,
                dst,
                dst_off,
                src,
                src_off,
                count,
            } => {
                let (es, _) = scalar_size(self.interner, *elem_ty);
                let d = self.operand(func, dst);
                let s = self.operand(func, src);
                let dof = self.operand(func, dst_off);
                let sof = self.operand(func, src_off);
                let n = self.operand(func, count);
                let db = self.tmp();
                let sb = self.tmp();
                let nb = self.tmp();
                let da = self.tmp();
                let sa = self.tmp();
                let _ = writeln!(self.buf, "  {} = mul i32 {}, {}", db, dof, es);
                let _ = writeln!(self.buf, "  {} = add i32 {}, 4", da, db);
                let da2 = self.tmp();
                let _ = writeln!(self.buf, "  {} = add i32 {}, {}", da2, d, da);
                let _ = writeln!(self.buf, "  {} = mul i32 {}, {}", sb, sof, es);
                let _ = writeln!(self.buf, "  {} = add i32 {}, 4", sa, sb);
                let sa2 = self.tmp();
                let _ = writeln!(self.buf, "  {} = add i32 {}, {}", sa2, s, sa);
                let _ = writeln!(self.buf, "  {} = mul i32 {}, {}", nb, n, es);
                let _ = writeln!(
                    self.buf,
                    "  call void @dream_memcpy(i32 {}, i32 {}, i32 {})",
                    da2, sa2, nb
                );
            }
            Statement::ForceFree(o) => {
                let v = self.operand(func, o);
                let _ = writeln!(self.buf, "  call void @dream_free(i32 {})", v);
            }
            Statement::LockAcquire(o) => {
                let v = self.operand(func, o);
                let _ = writeln!(self.buf, "  call void @dream_lock_acquire(i32 {})", v);
            }
            Statement::LockRelease(o) => {
                let v = self.operand(func, o);
                let _ = writeln!(self.buf, "  call void @dream_lock_release(i32 {})", v);
            }
            Statement::SimdF32x4 {
                op,
                dest,
                lhs,
                rhs,
                index,
            } => self.emit_simd(func, *op, dest, lhs, rhs, index),
            Statement::ValueDrop(l) => self.emit_value_drop(func, *l, true),
            Statement::ValueRetain(l) => self.emit_value_retain(func, *l),
            Statement::ValueKill(l) => self.emit_value_drop(func, *l, false),
        }
    }

    pub(crate) fn emit_print(&mut self, func: &MirFunction, arg: &Operand, ty: TypeId, newline: bool) {
        let v = self.operand(func, arg);
        let src = self.op_ty(func, arg);
        match self.interner.kind(ty) {
            TyKind::Prim(PrimTy::Int) | TyKind::Prim(PrimTy::Byte) | TyKind::Enum(_) => {
                let i = self.coerce(&v, src, "i32");
                let _ = writeln!(self.buf, "  call void @dream_print_int(i32 {})", i);
            }
            TyKind::Prim(PrimTy::UInt) => {
                let i = self.coerce(&v, src, "i32");
                let _ = writeln!(self.buf, "  call void @dream_print_uint(i32 {})", i);
            }
            TyKind::Prim(PrimTy::Long) => {
                let i = self.coerce(&v, src, "i64");
                let _ = writeln!(self.buf, "  call void @dream_print_long(i64 {})", i);
            }
            TyKind::Prim(PrimTy::ULong) => {
                let i = self.coerce(&v, src, "i64");
                let _ = writeln!(self.buf, "  call void @dream_print_ulong(i64 {})", i);
            }
            TyKind::Prim(PrimTy::Float) => {
                let f = self.coerce(&v, src, "float");
                let _ = writeln!(self.buf, "  call void @dream_print_float(float {})", f);
            }
            TyKind::Prim(PrimTy::Double) => {
                let f = self.coerce(&v, src, "double");
                let _ = writeln!(self.buf, "  call void @dream_print_double(double {})", f);
            }
            TyKind::Prim(PrimTy::Bool) => {
                let i = self.coerce(&v, src, "i32");
                let _ = writeln!(self.buf, "  call void @dream_print_bool(i32 {})", i);
            }
            TyKind::Prim(PrimTy::Char) => {
                let i = self.coerce(&v, src, "i32");
                let _ = writeln!(self.buf, "  call void @dream_print_char(i32 {})", i);
            }
            TyKind::Prim(PrimTy::String) => {
                let _ = writeln!(self.buf, "  call void @dream_print_string(i32 {})", v);
            }
            _ => {
                let s = self.emit_to_string(func, arg);
                let _ = writeln!(self.buf, "  call void @dream_print_string(i32 {})", s);
            }
        }
        if newline {
            self.buf.push_str("  call void @dream_print_newline()\n");
        }
    }

    pub(crate) fn emit_value_drop(&mut self, func: &MirFunction, local: dream_mir::Local, drop_fields: bool) {
        let ty = func.local_ty(local);
        if drop_fields && self.interner.is_value_type(ty) && self.mir.layouts.get(ty).is_some() {
            let v = {
                let t = self.tmp();
                let lty = llvm_val_ty(self.interner, ty);
                let _ = writeln!(self.buf, "  {} = load {}, {}* %l{}", t, lty, lty, local.0);
                t
            };
            let _ = writeln!(
                self.buf,
                "  call void @{}(i32 {})",
                fmt_sym("drop", ty),
                v
            );
        }
        let lty = llvm_val_ty(self.interner, ty);
        if lty != "void" {
            let _ = writeln!(self.buf, "  store {} {}, {}* %l{}", lty, zero(lty), lty, local.0);
        }
    }

    pub(crate) fn emit_value_retain(&mut self, func: &MirFunction, local: dream_mir::Local) {
        let ty = func.local_ty(local);
        if !self.interner.is_value_type(ty) {
            return;
        }
        let Some(layout) = self.mir.layouts.get(ty) else {
            return;
        };
        let base = {
            let t = self.tmp();
            let lty = llvm_val_ty(self.interner, ty);
            let _ = writeln!(self.buf, "  {} = load {}, {}* %l{}", t, lty, lty, local.0);
            t
        };
        for f in &layout.fields {
            if f.is_weak || f.is_unowned || !self.interner.is_reference(f.ty) {
                continue;
            }
            let addr = self.tmp();
            let _ = writeln!(self.buf, "  {} = add i32 {}, {}", addr, base, f.offset);
            let v = self.load_width(f.ty, &addr);
            let _ = writeln!(self.buf, "  call void @dream_retain(i32 {})", v);
        }
    }

    pub(crate) fn coerce(&mut self, v: &str, from: TypeId, to: &str) -> String {
        let ft = llvm_val_ty(self.interner, from);
        if ft == to {
            return v.to_string();
        }
        let t = self.tmp();
        match (ft, to) {
            ("i32", "float") => {
                if matches!(self.interner.kind(from), TyKind::Prim(PrimTy::UInt | PrimTy::Byte)) {
                    let _ = writeln!(self.buf, "  {} = uitofp i32 {} to float", t, v);
                } else {
                    let _ = writeln!(self.buf, "  {} = sitofp i32 {} to float", t, v);
                }
            }
            ("i32", "double") => {
                if matches!(self.interner.kind(from), TyKind::Prim(PrimTy::UInt | PrimTy::Byte)) {
                    let _ = writeln!(self.buf, "  {} = uitofp i32 {} to double", t, v);
                } else {
                    let _ = writeln!(self.buf, "  {} = sitofp i32 {} to double", t, v);
                }
            }
            ("i64", "float") => {
                if matches!(self.interner.kind(from), TyKind::Prim(PrimTy::ULong)) {
                    let _ = writeln!(self.buf, "  {} = uitofp i64 {} to float", t, v);
                } else {
                    let _ = writeln!(self.buf, "  {} = sitofp i64 {} to float", t, v);
                }
            }
            ("i64", "double") => {
                if matches!(self.interner.kind(from), TyKind::Prim(PrimTy::ULong)) {
                    let _ = writeln!(self.buf, "  {} = uitofp i64 {} to double", t, v);
                } else {
                    let _ = writeln!(self.buf, "  {} = sitofp i64 {} to double", t, v);
                }
            }
            ("i32", "i64") => {
                if matches!(self.interner.kind(from), TyKind::Prim(PrimTy::UInt | PrimTy::Byte)) {
                    let _ = writeln!(self.buf, "  {} = zext i32 {} to i64", t, v);
                } else {
                    let _ = writeln!(self.buf, "  {} = sext i32 {} to i64", t, v);
                }
            }
            ("i64", "i32") => {
                let _ = writeln!(self.buf, "  {} = trunc i64 {} to i32", t, v);
            }
            ("float", "double") => {
                let _ = writeln!(self.buf, "  {} = fpext float {} to double", t, v);
            }
            ("double", "float") => {
                let _ = writeln!(self.buf, "  {} = fptrunc double {} to float", t, v);
            }
            ("float", "i32") => {
                let _ = writeln!(self.buf, "  {} = fptosi float {} to i32", t, v);
            }
            ("double", "i32") => {
                let _ = writeln!(self.buf, "  {} = fptosi double {} to i32", t, v);
            }
            ("float", "i64") => {
                let _ = writeln!(self.buf, "  {} = fptosi float {} to i64", t, v);
            }
            ("double", "i64") => {
                let _ = writeln!(self.buf, "  {} = fptosi double {} to i64", t, v);
            }
            _ => return v.to_string(),
        }
        t
    }

    pub(crate) fn emit_term(&mut self, func: &MirFunction, term: &Terminator) {
        match term {
            Terminator::Goto(b) => {
                let _ = writeln!(self.buf, "  br label %bb{}", b.0);
            }
            Terminator::If {
                cond,
                then_blk,
                else_blk,
            } => {
                let c = self.operand(func, cond);
                let t = self.tmp();
                let _ = writeln!(self.buf, "  {} = icmp ne i32 {}, 0", t, c);
                let _ = writeln!(
                    self.buf,
                    "  br i1 {}, label %bb{}, label %bb{}",
                    t, then_blk.0, else_blk.0
                );
            }
            Terminator::Return(None) => {
                self.emit_debug_exit();
                if matches!(
                    self.interner.kind(llvm_fn_ret(self.interner, &self.mir.layouts, func)),
                    TyKind::Void | TyKind::Error
                ) {
                    self.buf.push_str("  ret void\n");
                } else {
                    let ty = llvm_val_ty(
                        self.interner,
                        llvm_fn_ret(self.interner, &self.mir.layouts, func),
                    );
                    let _ = writeln!(self.buf, "  ret {} {}", ty, zero(ty));
                }
            }
            Terminator::Return(Some(o)) => {
                self.emit_debug_exit();
                let ret_id = llvm_fn_ret(self.interner, &self.mir.layouts, func);
                let ty = llvm_val_ty(self.interner, ret_id);
                if matches!(self.interner.kind(ret_id), TyKind::Void) {
                    self.buf.push_str("  ret void\n");
                } else {
                    let v = self.operand(func, o);
                    let v = self.coerce(&v, self.op_ty(func, o), ty);
                    let _ = writeln!(self.buf, "  ret {} {}", ty, v);
                }
            }
            Terminator::Unreachable => self.buf.push_str("  unreachable\n"),
            Terminator::AsyncComplete(v) => {
                self.emit_debug_exit();
                let ret_id = llvm_fn_ret(self.interner, &self.mir.layouts, func);
                match v {
                    None => {
                        if matches!(self.interner.kind(ret_id), TyKind::Void | TyKind::Error) {
                            self.buf.push_str("  ret void\n");
                        } else {
                            let ty = llvm_val_ty(self.interner, ret_id);
                            let _ = writeln!(self.buf, "  ret {} {}", ty, zero(ty));
                        }
                    }
                    Some(o) => {
                        let ty = llvm_val_ty(self.interner, ret_id);
                        if matches!(self.interner.kind(ret_id), TyKind::Void) {
                            self.buf.push_str("  ret void\n");
                        } else {
                            let val = self.operand(func, o);
                            let val = self.coerce(&val, self.op_ty(func, o), ty);
                            let _ = writeln!(self.buf, "  ret {} {}", ty, val);
                        }
                    }
                }
            }
            Terminator::Switch {
                value,
                targets,
                default,
            } => {
                let v = self.operand(func, value);
                let mut arms = String::new();
                for (k, b) in targets {
                    let _ = write!(arms, " i32 {}, label %bb{}", k, b.0);
                }
                let _ = writeln!(
                    self.buf,
                    "  switch i32 {}, label %bb{} [{} ]",
                    v, default.0, arms
                );
            }
            Terminator::Await { future, dest, resume } => {
                let v = self.operand(func, future);
                if let Some(d) = dest {
                    let dest_ty = func.local_ty(*d);
                    let ty = llvm_val_ty(self.interner, dest_ty);
                    let v = self.coerce(&v, self.op_ty(func, future), ty);
                    let _ = writeln!(self.buf, "  store {} {}, {}* %l{}", ty, v, ty, d.0);
                }
                let _ = writeln!(self.buf, "  br label %bb{}", resume.0);
            }
            Terminator::TailCall { callee, args } => {
                let v = self.emit_call(func, callee, args, false);
                let ret_id = llvm_fn_ret_callee(self.interner, self.mir, callee);
                let ty = llvm_val_ty(self.interner, ret_id);
                if matches!(self.interner.kind(ret_id), TyKind::Void | TyKind::Error) {
                    self.buf.push_str("  ret void\n");
                } else {
                    let v = self.coerce(&v, ret_id, ty);
                    let _ = writeln!(self.buf, "  ret {} {}", ty, v);
                }
            }
        }
    }

}
