//! Calls, rvalues, casts, and binary/unary ops.

use super::ModuleEmitter;
use super::names::*;
use dream_hir::{scalar_size, BinOp, UnOp};
use dream_mir::func_symbol;
use dream_mir::{MirFunction, Operand, Rvalue};
use dream_types::{PrimTy, TyKind, TypeId};
use std::fmt::Write as _;

impl<'a> ModuleEmitter<'a> {
    pub(crate) fn emit_call(
        &mut self,
        func: &MirFunction,
        callee: &dream_mir::Callee,
        args: &[Operand],
        _drop: bool,
    ) -> String {
        let expected: Vec<TypeId> = self
            .mir
            .functions
            .iter()
            .find(|f| f.def == callee.def && f.instance == callee.args)
            .map(|f| f.params.iter().map(|p| f.local_ty(*p)).collect())
            .or_else(|| {
                self.mir
                    .imports
                    .iter()
                    .find(|imp| imp.def == callee.def)
                    .map(|imp| imp.params.clone())
            })
            .unwrap_or_default();
        let mut alist = Vec::new();
        let mut arg_tys = Vec::new();
        for (i, a) in args.iter().enumerate() {
            let from = self.op_ty(func, a);
            let raw = self.operand(func, a);
            let to_id = expected.get(i).copied().unwrap_or(from);
            let lty = llvm_val_ty(self.interner, to_id);
            let v = self.coerce(&raw, from, lty);
            arg_tys.push(lty.to_string());
            alist.push(format!("{} {}", lty, v));
        }
        let ret_id = llvm_fn_ret_callee(self.interner, self.mir, callee);
        let ret = if matches!(self.interner.kind(ret_id), TyKind::Void | TyKind::Error) {
            "void".to_string()
        } else {
            llvm_val_ty(self.interner, ret_id).to_string()
        };
        let name = self.resolve_callee(callee, &ret, &arg_tys);
        if ret == "void" {
            let _ = writeln!(self.buf, "  call void @{}({})", name, alist.join(", "));
            return "0".into();
        }
        let t = self.tmp();
        let _ = writeln!(
            self.buf,
            "  {} = call {} @{}({})",
            t,
            ret,
            name,
            alist.join(", ")
        );
        t
    }

    pub(crate) fn rvalue(&mut self, func: &MirFunction, rv: &Rvalue, dest_ty: TypeId) -> String {
        match rv {
            Rvalue::Use(o) => {
                let v = self.operand(func, o);
                self.coerce(
                    &v,
                    self.op_ty(func, o),
                    llvm_val_ty(self.interner, dest_ty),
                )
            }
            Rvalue::Binary(op, a, b) => {
                let v = self.binary(func, *op, a, b);
                if op.is_comparison() {
                    v
                } else {
                    self.coerce(
                        &v,
                        self.op_ty(func, a),
                        llvm_val_ty(self.interner, dest_ty),
                    )
                }
            }
            Rvalue::Unary(op, a) => self.unary(func, *op, a),
            Rvalue::Select {
                cond,
                then_val,
                else_val,
            } => {
                let c = self.operand(func, cond);
                let ty = llvm_val_ty(self.interner, dest_ty);
                let t_raw = self.operand(func, then_val);
                let t_from = self.op_ty(func, then_val);
                let t = self.coerce(&t_raw, t_from, ty);
                let e_raw = self.operand(func, else_val);
                let e_from = self.op_ty(func, else_val);
                let e = self.coerce(&e_raw, e_from, ty);
                let ic = self.tmp();
                let r = self.tmp();
                let _ = writeln!(self.buf, "  {} = icmp ne i32 {}, 0", ic, c);
                let _ = writeln!(self.buf, "  {} = select i1 {}, {} {}, {} {}", r, ic, ty, t, ty, e);
                r
            }
            Rvalue::Call { callee, args } => {
                let v = self.emit_call(func, callee, args, false);
                let rid = llvm_fn_ret_callee(self.interner, self.mir, callee);
                self.coerce(&v, rid, llvm_val_ty(self.interner, dest_ty))
            }
            Rvalue::StrLen(o) => {
                let v = self.operand(func, o);
                let t = self.tmp();
                let _ = writeln!(self.buf, "  {} = call i32 @dream_str_scalar_len(i32 {})", t, v);
                t
            }
            Rvalue::StrByteSize(o) => {
                let v = self.operand(func, o);
                let t = self.tmp();
                let _ = writeln!(self.buf, "  {} = call i32 @dream_str_byte_size(i32 {})", t, v);
                t
            }
            Rvalue::Concat(a, b) => {
                let x = self.operand(func, a);
                let y = self.operand(func, b);
                let t = self.tmp();
                let _ = writeln!(
                    self.buf,
                    "  {} = call i32 @dream_concat_strings(i32 {}, i32 {})",
                    t, x, y
                );
                t
            }
            Rvalue::CharAt(s, i) => {
                let x = self.operand(func, s);
                let y = self.operand(func, i);
                let t = self.tmp();
                let _ = writeln!(self.buf, "  {} = call i32 @dream_char_at(i32 {}, i32 {})", t, x, y);
                t
            }
            Rvalue::ByteAt(s, i) => {
                let x = self.operand(func, s);
                let y = self.operand(func, i);
                let t = self.tmp();
                let _ = writeln!(self.buf, "  {} = call i32 @dream_byte_at(i32 {}, i32 {})", t, x, y);
                t
            }
            Rvalue::ArrayLen(o) => {
                let v = self.operand(func, o);
                let t = self.tmp();
                let _ = writeln!(self.buf, "  {} = call i32 @dream_array_len(i32 {})", t, v);
                t
            }
            Rvalue::New { ty, ctor, args, .. } => self.emit_new(func, *ty, *ctor, args),
            Rvalue::ArrayNew { elem_ty, len } => {
                let n = self.operand(func, len);
                let (es, _) = scalar_size(self.interner, *elem_ty);
                let bytes = self.tmp();
                let _ = writeln!(self.buf, "  {} = mul i32 {}, {}", bytes, n, es);
                let tot = self.tmp();
                let _ = writeln!(self.buf, "  {} = add i32 {}, 4", tot, bytes);
                let tag = self.array_tag(*elem_ty);
                let p = self.tmp();
                let _ = writeln!(
                    self.buf,
                    "  {} = call i32 @dream_malloc(i32 {}, i32 {})",
                    p, tot, tag
                );
                let _ = writeln!(self.buf, "  call void @dream_store_i32(i32 {}, i32 {})", p, n);
                p
            }
            Rvalue::ArrayLit { elem_ty, elems } => {
                let n = elems.len() as i32;
                let (es, _) = scalar_size(self.interner, *elem_ty);
                let payload = 4 + n * es as i32;
                let tag = self.array_tag(*elem_ty);
                let p = self.tmp();
                let _ = writeln!(
                    self.buf,
                    "  {} = call i32 @dream_malloc(i32 {}, i32 {})",
                    p, payload, tag
                );
                let _ = writeln!(self.buf, "  call void @dream_store_i32(i32 {}, i32 {})", p, n);
                for (i, el) in elems.iter().enumerate() {
                    let raw = self.operand(func, el);
                    let from = self.op_ty(func, el);
                    let v = self.coerce(&raw, from, llvm_val_ty(self.interner, *elem_ty));
                    let addr = self.tmp();
                    let _ = writeln!(
                        self.buf,
                        "  {} = add i32 {}, {}",
                        addr,
                        p,
                        4 + i as i32 * es as i32
                    );
                    self.retain_if_ref(*elem_ty, &v);
                    self.store_width(*elem_ty, &addr, &v);
                }
                p
            }
            Rvalue::Cast(o, _from, to) => {
                let v = self.operand(func, o);
                self.cast(func, o, v, *to)
            }
            Rvalue::FuncRef(c) => format!("{}", self.func_index(c)),
            Rvalue::IndirectCall { target, args, sig } => {
                self.emit_indirect(func, target, args, *sig, false)
            }
            Rvalue::InterfaceCall {
                receiver,
                iface_id,
                method_slot,
                args,
                ..
            } => self.emit_iface(func, receiver, *iface_id, *method_slot, args, false),
            Rvalue::JsCall {
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
                self.emit_js_vals(callee, vals, false)
            }
            Rvalue::Discriminant(o) => {
                let p = self.operand(func, o);
                let t = self.tmp();
                let _ = writeln!(self.buf, "  {} = call i32 @dream_load_i32(i32 {})", t, p);
                t
            }
            Rvalue::IsType(o, ty) => {
                let p = self.operand(func, o);
                let tag = runtime_tag(self.interner, *ty, &self.tags);
                let got = self.tmp();
                let _ = writeln!(self.buf, "  {} = call i32 @dream_object_tag(i32 {})", got, p);
                let c = self.tmp();
                let r = self.tmp();
                let _ = writeln!(self.buf, "  {} = icmp eq i32 {}, {}", c, got, tag);
                let _ = writeln!(self.buf, "  {} = zext i1 {} to i32", r, c);
                r
            }
            Rvalue::UnionNew { ty, variant, args, .. } => {
                self.emit_union_new(func, *ty, *variant, args)
            }
            Rvalue::UnionField {
                base,
                ty,
                variant,
                field,
            } => self.emit_union_field(func, base, *ty, *variant, *field),
            Rvalue::Tuple { ty, elems } => self.emit_tuple(func, *ty, elems),
            Rvalue::HashCode(o) => self.emit_hash(func, o),
            Rvalue::ToString(o) => self.emit_to_string(func, o),
            Rvalue::EnumName { value, arms } => self.emit_enum_name(func, value, arms),
            Rvalue::ToBytes { value, ty } => self.emit_to_bytes(func, value, *ty),
            Rvalue::FromBytes { bytes, ty } => self.emit_from_bytes(func, bytes, *ty),
            Rvalue::ArrayRealloc {
                elem_ty,
                array,
                new_len,
            } => self.emit_realloc(func, *elem_ty, array, new_len),
        }
    }

    pub(crate) fn emit_new(
        &mut self,
        func: &MirFunction,
        ty: TypeId,
        ctor: Option<dream_types::DefId>,
        args: &[Operand],
    ) -> String {
        let size = self
            .mir
            .layouts
            .get(ty)
            .map(|l| l.size)
            .unwrap_or(0);
        let tag = self.tags.get(&ty).copied().unwrap_or(dream_mir::abi::TAG_STRUCT_BASE);
        let p = self.tmp();
        let _ = writeln!(
            self.buf,
            "  {} = call i32 @dream_malloc(i32 {}, i32 {})",
            p, size, tag
        );
        if size > 0 {
            let _ = writeln!(self.buf, "  call void @dream_memzero(i32 {}, i32 {})", p, size);
        }
        if let Some(def) = ctor {
            let mut cargs = vec![p.clone()];
            for a in args {
                cargs.push(self.operand(func, a));
            }
            let mut found = None;
            for f in &self.mir.functions {
                if f.def == def {
                    found = Some(llvm_fn_name(&func_symbol(f)));
                    break;
                }
            }
            if let Some(name) = found {
                let mut alist = Vec::new();
                alist.push(format!("i32 {}", cargs[0]));
                for (i, a) in args.iter().enumerate() {
                    let ty = self.op_ty(func, a);
                    alist.push(format!("{} {}", llvm_val_ty(self.interner, ty), cargs[i + 1]));
                }
                let _ = writeln!(self.buf, "  call void @{}({})", name, alist.join(", "));
            }
        }
        p
    }

    pub(crate) fn cast(&mut self, func: &MirFunction, o: &Operand, v: String, to: TypeId) -> String {
        let from = self.op_ty(func, o);
        if let Some(boxed) = self.try_box(from, to, &v) {
            return boxed;
        }
        if let Some(unboxed) = self.try_unbox(from, to, &v) {
            return unboxed;
        }
        self.coerce(&v, from, llvm_val_ty(self.interner, to))
    }

    pub(crate) fn try_box(&mut self, from: TypeId, to: TypeId, v: &str) -> Option<String> {
        let to_obj = matches!(
            self.interner.kind(to),
            TyKind::Object | TyKind::Interface(..)
        );
        if !to_obj {
            return None;
        }
        let t = self.tmp();
        match self.interner.kind(from) {
            TyKind::Prim(PrimTy::Int | PrimTy::UInt | PrimTy::Byte | PrimTy::Bool | PrimTy::Char)
            | TyKind::Enum(_) => {
                let tag = runtime_tag(self.interner, from, &self.tags);
                let _ = writeln!(
                    self.buf,
                    "  {} = call i32 @dream_box_i32(i32 {}, i32 {})",
                    t, v, tag
                );
                Some(t)
            }
            TyKind::Prim(PrimTy::Long | PrimTy::ULong) => {
                let tag = runtime_tag(self.interner, from, &self.tags);
                let _ = writeln!(
                    self.buf,
                    "  {} = call i32 @dream_box_i64(i64 {}, i32 {})",
                    t, v, tag
                );
                Some(t)
            }
            TyKind::Prim(PrimTy::Float) => {
                let _ = writeln!(self.buf, "  {} = call i32 @dream_box_f32(float {})", t, v);
                Some(t)
            }
            TyKind::Prim(PrimTy::Double) => {
                let _ = writeln!(self.buf, "  {} = call i32 @dream_box_f64(double {})", t, v);
                Some(t)
            }
            _ => None,
        }
    }

    pub(crate) fn try_unbox(&mut self, from: TypeId, to: TypeId, v: &str) -> Option<String> {
        let from_obj = matches!(
            self.interner.kind(from),
            TyKind::Object | TyKind::Interface(..)
        );
        if !from_obj {
            return None;
        }
        let t = self.tmp();
        match self.interner.kind(to) {
            TyKind::Prim(PrimTy::Int | PrimTy::UInt | PrimTy::Byte | PrimTy::Bool | PrimTy::Char)
            | TyKind::Enum(_) => {
                let _ = writeln!(self.buf, "  {} = call i32 @dream_unbox_i32(i32 {})", t, v);
                Some(t)
            }
            TyKind::Prim(PrimTy::Long | PrimTy::ULong) => {
                let _ = writeln!(self.buf, "  {} = call i64 @dream_unbox_i64(i32 {})", t, v);
                Some(t)
            }
            TyKind::Prim(PrimTy::Float) => {
                let _ = writeln!(self.buf, "  {} = call float @dream_unbox_f32(i32 {})", t, v);
                Some(t)
            }
            TyKind::Prim(PrimTy::Double) => {
                let _ = writeln!(self.buf, "  {} = call double @dream_unbox_f64(i32 {})", t, v);
                Some(t)
            }
            _ => None,
        }
    }

    pub(crate) fn binary(&mut self, func: &MirFunction, op: BinOp, a: &Operand, b: &Operand) -> String {
        let ta = self.op_ty(func, a);
        if matches!(op, BinOp::Eq | BinOp::Ne)
            && matches!(self.interner.kind(ta), TyKind::Prim(PrimTy::String))
        {
            let x = self.operand(func, a);
            let y = self.operand(func, b);
            let t = self.tmp();
            let _ = writeln!(self.buf, "  {} = call i32 @dream_string_eq(i32 {}, i32 {})", t, x, y);
            if matches!(op, BinOp::Ne) {
                let u = self.tmp();
                let _ = writeln!(self.buf, "  {} = xor i32 {}, 1", u, t);
                return u;
            }
            return t;
        }
        let x = self.operand(func, a);
        let y = self.operand(func, b);
        let w = llvm_val_ty(self.interner, ta);
        let t = self.tmp();
        let instr = match (op, w) {
            (BinOp::Add, "float" | "double") => "fadd",
            (BinOp::Sub, "float" | "double") => "fsub",
            (BinOp::Mul, "float" | "double") => "fmul",
            (BinOp::Div, "float" | "double") => "fdiv",
            (BinOp::Add, _) => "add",
            (BinOp::Sub, _) => "sub",
            (BinOp::Mul, _) => "mul",
            (BinOp::Div, _) => "sdiv",
            (BinOp::Rem, _) => "srem",
            (BinOp::BitAnd | BinOp::And, _) => "and",
            (BinOp::BitOr | BinOp::Or, _) => "or",
            (BinOp::BitXor, _) => "xor",
            (BinOp::Shl, _) => "shl",
            (BinOp::Shr, _) => "ashr",
            (BinOp::Eq, "float" | "double") => "fcmp oeq",
            (BinOp::Ne, "float" | "double") => "fcmp one",
            (BinOp::Lt, "float" | "double") => "fcmp olt",
            (BinOp::Le, "float" | "double") => "fcmp ole",
            (BinOp::Gt, "float" | "double") => "fcmp ogt",
            (BinOp::Ge, "float" | "double") => "fcmp oge",
            (BinOp::Eq, _) => "icmp eq",
            (BinOp::Ne, _) => "icmp ne",
            (BinOp::Lt, _) => "icmp slt",
            (BinOp::Le, _) => "icmp sle",
            (BinOp::Gt, _) => "icmp sgt",
            (BinOp::Ge, _) => "icmp sge",
        };
        if op.is_comparison() {
            let c = self.tmp();
            let _ = writeln!(self.buf, "  {} = {} {} {}, {}", c, instr, w, x, y);
            let _ = writeln!(self.buf, "  {} = zext i1 {} to i32", t, c);
        } else {
            let _ = writeln!(self.buf, "  {} = {} {} {}, {}", t, instr, w, x, y);
        }
        t
    }

    pub(crate) fn unary(&mut self, func: &MirFunction, op: UnOp, a: &Operand) -> String {
        let x = self.operand(func, a);
        let ta = self.op_ty(func, a);
        let w = llvm_val_ty(self.interner, ta);
        let t = self.tmp();
        match op {
            UnOp::Neg if w == "float" || w == "double" => {
                let _ = writeln!(self.buf, "  {} = fneg {} {}", t, w, x);
            }
            UnOp::Neg => {
                let _ = writeln!(self.buf, "  {} = sub {} {}, {}", t, w, zero(w), x);
            }
            UnOp::Not => {
                let _ = writeln!(self.buf, "  {} = icmp eq i32 {}, 0", t, x);
                let u = self.tmp();
                let _ = writeln!(self.buf, "  {} = zext i1 {} to i32", u, t);
                return u;
            }
            UnOp::BitNot => {
                let _ = writeln!(self.buf, "  {} = xor {} {}, -1", t, w, x);
            }
        }
        t
    }
}
