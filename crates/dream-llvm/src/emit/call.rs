//! Imports, indirect/interface calls, unions, stringify.

use super::ModuleEmitter;
use super::names::*;
use dream_hir::{scalar_size, BinOp};
use dream_mir::func_symbol;
use dream_mir::{Callee, MirFunction, Operand};
use dream_types::{PrimTy, TyKind, TypeId};
use std::fmt::Write as _;

impl<'a> ModuleEmitter<'a> {
    pub(crate) fn func_index(&self, c: &dream_mir::Callee) -> i32 {
        self.mir
            .functions
            .iter()
            .position(|f| f.def == c.def && f.instance == c.args)
            .unwrap_or(0) as i32
    }

    pub(crate) fn emit_import_stubs(&mut self) {
        for imp in &self.mir.imports {
            if is_c_runtime_sym(&imp.name)
                || native_c_sym(&imp.name).is_some()
                || native_c_sym(&imp.field).is_some()
            {
                continue;
            }
            let name = llvm_extern_name(&imp.name);
            let ret = imp
                .ret
                .map(|t| llvm_val_ty(self.interner, t))
                .unwrap_or("void");
            let mut args = Vec::new();
            for (i, p) in imp.params.iter().enumerate() {
                args.push(format!("{} %a{}", llvm_val_ty(self.interner, *p), i));
            }
            if ret == "void" {
                let _ = writeln!(self.buf, "\ndefine void @{}({}) {{", name, args.join(", "));
            } else {
                let _ = writeln!(
                    self.buf,
                    "\ndefine {} @{}({}) {{",
                    ret,
                    name,
                    args.join(", ")
                );
            }
            let label = if imp.field.is_empty() {
                imp.name.as_str()
            } else {
                imp.field.as_str()
            };
            let msg = self.intern_cstr(label);
            let _ = writeln!(self.buf, "  call void @dream_unimplemented(i8* {})", msg);
            if ret == "void" {
                self.buf.push_str("  ret void\n}\n");
            } else {
                let _ = writeln!(self.buf, "  ret {} {}\n}}\n", ret, zero(ret));
            }
        }
    }

    pub(crate) fn emit_indirect(
        &mut self,
        func: &MirFunction,
        target: &Operand,
        args: &[Operand],
        sig: TypeId,
        drop: bool,
    ) -> String {
        let raw = self.operand(func, target);
        let tgt_ty = self.op_ty(func, target);
        let idx = if matches!(self.interner.kind(tgt_ty), TyKind::Func(..)) {
            let t = self.tmp();
            let _ = writeln!(
                self.buf,
                "  {} = call i32 @d_funcbox_funcidx(i32 {})",
                t, raw
            );
            t
        } else {
            raw
        };
        let ret_ty = match self.interner.kind(sig) {
            TyKind::Func(_, ret) => *ret,
            _ => self.interner.void(),
        };
        let lty = llvm_val_ty(self.interner, ret_ty);
        let is_void = drop || matches!(self.interner.kind(ret_ty), TyKind::Void | TyKind::Error);
        let cands: Vec<(i32, dream_types::DefId, Vec<TypeId>, TypeId)> = self
            .mir
            .functions
            .iter()
            .enumerate()
            .filter(|(_, f)| f.params.len() == args.len())
            .map(|(i, f)| (i as i32, f.def, f.instance.clone(), f.ret))
            .collect();
        let join = format!("indj{}", self.next);
        self.next += 1;
        let miss = format!("indm{}", self.next);
        self.next += 1;
        let mut sw = String::new();
        let mut arms = Vec::new();
        for (i, def, inst, ret) in &cands {
            let lab = format!("indc{}_{}", i, self.next);
            self.next += 1;
            let _ = write!(sw, " i32 {}, label %{}", i, lab);
            arms.push((
                lab,
                dream_mir::Callee {
                    def: *def,
                    args: inst.clone(),
                    ret: *ret,
                    take_params: vec![],
                },
            ));
        }
        let _ = writeln!(
            self.buf,
            "  switch i32 {}, label %{} [{}]",
            idx, miss, sw
        );
        let mut phi = Vec::new();
        for (lab, callee) in arms {
            let _ = writeln!(self.buf, "{}:", lab);
            let v = self.emit_call(func, &callee, args, is_void);
            if !is_void {
                phi.push((v, lab.clone()));
            }
            let _ = writeln!(self.buf, "  br label %{}", join);
        }
        let _ = writeln!(self.buf, "{}:", miss);
        let _ = writeln!(self.buf, "  br label %{}", join);
        let _ = writeln!(self.buf, "{}:", join);
        if is_void {
            "0".into()
        } else {
            phi.push((zero(lty).to_string(), miss));
            let r = self.tmp();
            let _ = writeln!(self.buf, "  {} = phi {} {}", r, lty, format_phi(&phi));
            r
        }
    }

    pub(crate) fn emit_js_vals(&mut self, callee: &Callee, ops: Vec<String>, drop: bool) -> String {
        let ret_id = callee.ret;
        let ret = if drop || matches!(self.interner.kind(ret_id), TyKind::Void | TyKind::Error) {
            "void".to_string()
        } else {
            llvm_val_ty(self.interner, ret_id).to_string()
        };
        let arg_tys: Vec<String> = ops.iter().map(|_| "i32".into()).collect();
        let name = self.resolve_callee(callee, &ret, &arg_tys);
        let alist: Vec<String> = ops.iter().map(|v| format!("i32 {}", v)).collect();
        if ret == "void" {
            let _ = writeln!(self.buf, "  call void @{}({})", name, alist.join(", "));
            "0".into()
        } else {
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
    }

    pub(crate) fn emit_iface(
        &mut self,
        func: &MirFunction,
        receiver: &Operand,
        iface_id: usize,
        method_slot: usize,
        args: &[Operand],
        drop: bool,
    ) -> String {
        let recv = self.operand(func, receiver);
        let tagv = self.tmp();
        let _ = writeln!(
            self.buf,
            "  {} = call i32 @dream_object_tag(i32 {})",
            tagv, recv
        );
        let ret_ty = self
            .mir
            .interfaces
            .interfaces
            .get(iface_id)
            .and_then(|i| i.sigs.get(method_slot))
            .map(|sig| match self.interner.kind(*sig) {
                TyKind::Func(_, r) => llvm_val_ty(self.interner, *r),
                _ => "i32",
            })
            .unwrap_or("i32");
        let is_void = drop || ret_ty == "void";
        let join = format!("ifj{}", self.next);
        self.next += 1;
        let defl = format!("ifd{}", self.next);
        self.next += 1;
        let mut arms = Vec::new();
        let mut phi = Vec::new();
        let mut arg_vals = Vec::new();
        for a in args {
            let ty = self.op_ty(func, a);
            let v = self.operand(func, a);
            arg_vals.push(format!("{} {}", llvm_val_ty(self.interner, ty), v));
        }
        for (i, imp) in self.mir.interfaces.impls.iter().enumerate() {
            let Some(syms) = imp
                .entries
                .iter()
                .find(|(id, _)| *id == iface_id)
                .map(|(_, s)| s.clone())
            else {
                continue;
            };
            if method_slot >= syms.len() {
                continue;
            }
            let tag = self.tags.get(&imp.class_ty).copied().unwrap_or(0);
            let lab = format!("ifi{}_{}", i, self.next);
            self.next += 1;
            arms.push((tag, lab.clone(), llvm_fn_name(&syms[method_slot])));
        }
        let mut sw = String::new();
        for (tag, lab, _) in &arms {
            let _ = write!(sw, " i32 {}, label %{}", tag, lab);
        }
        let _ = writeln!(
            self.buf,
            "  switch i32 {}, label %{} [{}]",
            tagv, defl, sw
        );
        for (tag, lab, name) in &arms {
            let _ = tag;
            let _ = writeln!(self.buf, "{}:", lab);
            let mut alist = vec![format!("i32 {}", recv)];
            alist.extend(arg_vals.iter().cloned());
            if is_void {
                let _ = writeln!(self.buf, "  call void @{}({})", name, alist.join(", "));
            } else {
                let t = self.tmp();
                let _ = writeln!(
                    self.buf,
                    "  {} = call {} @{}({})",
                    t,
                    ret_ty,
                    name,
                    alist.join(", ")
                );
                phi.push((t, lab.clone()));
            }
            let _ = writeln!(self.buf, "  br label %{}", join);
        }
        let _ = writeln!(self.buf, "{}:", defl);
        let _ = writeln!(self.buf, "  br label %{}", join);
        let _ = writeln!(self.buf, "{}:", join);
        if is_void {
            "0".into()
        } else {
            let r = self.tmp();
            phi.push((zero(ret_ty).to_string(), defl));
            let _ = writeln!(self.buf, "  {} = phi {} {}", r, ret_ty, format_phi(&phi));
            r
        }
    }

    pub(crate) fn emit_union_new(
        &mut self,
        func: &MirFunction,
        ty: TypeId,
        variant: usize,
        args: &[Operand],
    ) -> String {
        let layout = self.mir.layouts.union(ty);
        let size = layout.map(|l| l.size).unwrap_or(4);
        let disc = layout
            .and_then(|l| l.variants.get(variant))
            .map(|v| v.discriminant)
            .unwrap_or(variant as i32);
        let tag = self.tags.get(&ty).copied().unwrap_or(dream_mir::abi::TAG_STRUCT_BASE);
        let p = self.tmp();
        let _ = writeln!(
            self.buf,
            "  {} = call i32 @dream_malloc(i32 {}, i32 {})",
            p, size, tag
        );
        let _ = writeln!(self.buf, "  call void @dream_store_i32(i32 {}, i32 {})", p, disc);
        if let Some(var) = layout.and_then(|l| l.variants.get(variant)) {
            for (i, a) in args.iter().enumerate() {
                if let Some(f) = var.fields.get(i) {
                    let v = self.operand(func, a);
                    let addr = self.tmp();
                    let _ = writeln!(self.buf, "  {} = add i32 {}, {}", addr, p, f.offset);
                    self.store_width(f.ty, &addr, &v);
                }
            }
        }
        p
    }

    pub(crate) fn emit_union_field(
        &mut self,
        func: &MirFunction,
        base: &Operand,
        ty: TypeId,
        variant: usize,
        field: usize,
    ) -> String {
        let p = self.operand(func, base);
        let fty = self
            .mir
            .layouts
            .union(ty)
            .and_then(|l| l.variants.get(variant))
            .and_then(|v| v.fields.get(field));
        let off = fty.map(|f| f.offset).unwrap_or(4);
        let fty = fty.map(|f| f.ty).unwrap_or_else(|| self.interner.int());
        let addr = self.tmp();
        let _ = writeln!(self.buf, "  {} = add i32 {}, {}", addr, p, off);
        self.load_width(fty, &addr)
    }

    pub(crate) fn emit_tuple(&mut self, func: &MirFunction, ty: TypeId, elems: &[Operand]) -> String {
        let size = self.mir.layouts.get(ty).map(|l| l.size).unwrap_or(4 * elems.len() as u32);
        let tag = self.tags.get(&ty).copied().unwrap_or(dream_mir::abi::TAG_STRUCT_BASE);
        let p = self.tmp();
        let _ = writeln!(
            self.buf,
            "  {} = call i32 @dream_malloc(i32 {}, i32 {})",
            p, size, tag
        );
        if let Some(layout) = self.mir.layouts.get(ty) {
            for (i, el) in elems.iter().enumerate() {
                if let Some(f) = layout.fields.get(i) {
                    let v = self.operand(func, el);
                    let addr = self.tmp();
                    let _ = writeln!(self.buf, "  {} = add i32 {}, {}", addr, p, f.offset);
                    self.store_width(f.ty, &addr, &v);
                }
            }
        }
        p
    }

    pub(crate) fn emit_hash(&mut self, func: &MirFunction, o: &Operand) -> String {
        let ty = self.op_ty(func, o);
        let v = self.operand(func, o);
        match self.interner.kind(ty) {
            TyKind::Prim(PrimTy::String) => {
                let t = self.tmp();
                let _ = writeln!(self.buf, "  {} = call i32 @dream_hash_bytes(i32 {})", t, v);
                t
            }
            TyKind::Struct(..) => {
                if let Some(name) = self.override_sym(ty, "hash_code") {
                    let t = self.tmp();
                    let _ = writeln!(self.buf, "  {} = call i32 @{}(i32 {})", t, name, v);
                    t
                } else {
                    let t = self.tmp();
                    let _ = writeln!(
                        self.buf,
                        "  {} = call i32 @{}(i32 {})",
                        t,
                        fmt_sym("hash", ty),
                        v
                    );
                    t
                }
            }
            TyKind::Object | TyKind::Interface(..) => {
                let t = self.tmp();
                let _ = writeln!(self.buf, "  {} = call i32 @dream_object_hash(i32 {})", t, v);
                t
            }
            _ => v,
        }
    }

    pub(crate) fn emit_to_string(&mut self, func: &MirFunction, o: &Operand) -> String {
        let ty = self.op_ty(func, o);
        let v = self.operand(func, o);
        self.stringify_val(ty, &v)
    }

    pub(crate) fn stringify_val(&mut self, ty: TypeId, v: &str) -> String {
        let t = self.tmp();
        match self.interner.kind(ty) {
            TyKind::Prim(PrimTy::Long | PrimTy::ULong) => {
                let _ = writeln!(self.buf, "  {} = call i32 @dream_i64_to_string(i64 {})", t, v);
                t
            }
            TyKind::Prim(PrimTy::Float) => {
                let _ = writeln!(self.buf, "  {} = call i32 @dream_f32_to_string(float {})", t, v);
                t
            }
            TyKind::Prim(PrimTy::Double) => {
                let _ = writeln!(self.buf, "  {} = call i32 @dream_f64_to_string(double {})", t, v);
                t
            }
            TyKind::Prim(PrimTy::Bool) => {
                let _ = writeln!(self.buf, "  {} = call i32 @dream_bool_to_string(i32 {})", t, v);
                t
            }
            TyKind::Prim(PrimTy::String) => v.to_string(),
            TyKind::Struct(..) => {
                if let Some(name) = self.override_sym(ty, "to_string") {
                    let _ = writeln!(self.buf, "  {} = call i32 @{}(i32 {})", t, name, v);
                } else {
                    let _ = writeln!(
                        self.buf,
                        "  {} = call i32 @{}(i32 {})",
                        t,
                        fmt_sym("fmt", ty),
                        v
                    );
                }
                t
            }
            TyKind::Array(elem) => {
                let _ = writeln!(
                    self.buf,
                    "  {} = call i32 @{}(i32 {})",
                    t,
                    fmt_sym("afmt", *elem),
                    v
                );
                t
            }
            TyKind::Union(..) => {
                let _ = writeln!(
                    self.buf,
                    "  {} = call i32 @{}(i32 {})",
                    t,
                    fmt_sym("ufmt", ty),
                    v
                );
                t
            }
            TyKind::Object | TyKind::Interface(..) => {
                let _ = writeln!(self.buf, "  {} = call i32 @dream_object_to_string(i32 {})", t, v);
                t
            }
            _ => {
                let _ = writeln!(self.buf, "  {} = call i32 @dream_i32_to_string(i32 {})", t, v);
                t
            }
        }
    }

    pub(crate) fn override_sym(&self, ty: TypeId, method: &str) -> Option<String> {
        let layout = self.mir.layouts.get(ty)?;
        let want = dream_types::method_fn(&layout.name, method);
        self.mir
            .functions
            .iter()
            .find(|f| f.name == want)
            .map(|f| llvm_fn_name(&func_symbol(f)))
    }

    pub(crate) fn emit_enum_name(&mut self, func: &MirFunction, value: &Operand, arms: &[(i64, String)]) -> String {
        let v = self.operand(func, value);
        let join = format!("enj{}", self.next);
        self.next += 1;
        let defl = format!("endf{}", self.next);
        self.next += 1;
        let empty = self.intern_lit("");
        let mut sw = String::new();
        let mut cases = Vec::new();
        for (k, name) in arms {
            let lab = format!("en{}_{}", k, self.next);
            self.next += 1;
            let s = self.intern_lit(name);
            let _ = write!(sw, " i32 {}, label %{}", *k as i32, lab);
            cases.push((lab, s));
        }
        let _ = writeln!(self.buf, "  switch i32 {}, label %{} [{}]", v, defl, sw);
        let mut phi = Vec::new();
        for (lab, s) in cases {
            let _ = writeln!(self.buf, "{}:", lab);
            let _ = writeln!(self.buf, "  br label %{}", join);
            phi.push((s, lab));
        }
        let _ = writeln!(self.buf, "{}:", defl);
        let _ = writeln!(self.buf, "  br label %{}", join);
        phi.push((empty, defl));
        let _ = writeln!(self.buf, "{}:", join);
        let r = self.tmp();
        let _ = writeln!(self.buf, "  {} = phi i32 {}", r, format_phi(&phi));
        r
    }

    pub(crate) fn emit_to_bytes(&mut self, func: &MirFunction, value: &Operand, ty: TypeId) -> String {
        let (es, _) = scalar_size(self.interner, ty);
        let v = self.operand(func, value);
        let payload = 4 + es as i32;
        let p = self.tmp();
        let _ = writeln!(
            self.buf,
            "  {} = call i32 @dream_malloc(i32 {}, i32 {})",
            p,
            payload,
            dream_mir::abi::TAG_ARRAY
        );
        let _ = writeln!(self.buf, "  call void @dream_store_i32(i32 {}, i32 1)", p);
        let addr = self.tmp();
        let _ = writeln!(self.buf, "  {} = add i32 {}, 4", addr, p);
        self.store_width(ty, &addr, &v);
        p
    }

    pub(crate) fn emit_from_bytes(&mut self, func: &MirFunction, bytes: &Operand, ty: TypeId) -> String {
        let (es, _) = scalar_size(self.interner, ty);
        let b = self.operand(func, bytes);
        let src = self.tmp();
        let _ = writeln!(self.buf, "  {} = add i32 {}, 4", src, b);
        let tag = self.tags.get(&ty).copied().unwrap_or(dream_mir::abi::TAG_INT);
        let p = self.tmp();
        let _ = writeln!(
            self.buf,
            "  {} = call i32 @dream_malloc(i32 {}, i32 {})",
            p, es, tag
        );
        let _ = writeln!(
            self.buf,
            "  call void @dream_memcpy(i32 {}, i32 {}, i32 {})",
            p, src, es
        );
        let _ = func;
        p
    }

    pub(crate) fn emit_realloc(
        &mut self,
        func: &MirFunction,
        elem_ty: TypeId,
        array: &Operand,
        new_len: &Operand,
    ) -> String {
        let (es, _) = scalar_size(self.interner, elem_ty);
        let a = self.operand(func, array);
        let n = self.operand(func, new_len);
        let bytes = self.tmp();
        let tot = self.tmp();
        let _ = writeln!(self.buf, "  {} = mul i32 {}, {}", bytes, n, es);
        let _ = writeln!(self.buf, "  {} = add i32 {}, 4", tot, bytes);
        let tag = self.array_tag(elem_ty);
        let p = self.tmp();
        let _ = writeln!(
            self.buf,
            "  {} = call i32 @dream_realloc(i32 {}, i32 {}, i32 {})",
            p, a, tot, tag
        );
        let _ = writeln!(self.buf, "  call void @dream_store_i32(i32 {}, i32 {})", p, n);
        p
    }

    pub(crate) fn emit_simd(
        &mut self,
        func: &MirFunction,
        op: dream_hir::BinOp,
        dest: &Operand,
        lhs: &Operand,
        rhs: &Operand,
        index: &Operand,
    ) {
        let d = self.operand(func, dest);
        let l = self.operand(func, lhs);
        let r = self.operand(func, rhs);
        let i = self.operand(func, index);
        let off = self.tmp();
        let _ = writeln!(self.buf, "  {} = mul i32 {}, 16", off, i);
        let base_d = self.tmp();
        let base_l = self.tmp();
        let base_r = self.tmp();
        let _ = writeln!(self.buf, "  {} = add i32 {}, 4", base_d, d);
        let _ = writeln!(self.buf, "  {} = add i32 {}, 4", base_l, l);
        let _ = writeln!(self.buf, "  {} = add i32 {}, 4", base_r, r);
        let ad = self.tmp();
        let al = self.tmp();
        let ar = self.tmp();
        let _ = writeln!(self.buf, "  {} = add i32 {}, {}", ad, base_d, off);
        let _ = writeln!(self.buf, "  {} = add i32 {}, {}", al, base_l, off);
        let _ = writeln!(self.buf, "  {} = add i32 {}, {}", ar, base_r, off);
        for k in 0..4 {
            let a1 = self.tmp();
            let a2 = self.tmp();
            let a3 = self.tmp();
            let _ = writeln!(self.buf, "  {} = add i32 {}, {}", a1, al, k * 4);
            let _ = writeln!(self.buf, "  {} = add i32 {}, {}", a2, ar, k * 4);
            let _ = writeln!(self.buf, "  {} = add i32 {}, {}", a3, ad, k * 4);
            let x = self.tmp();
            let y = self.tmp();
            let _ = writeln!(self.buf, "  {} = call float @dream_load_f32(i32 {})", x, a1);
            let _ = writeln!(self.buf, "  {} = call float @dream_load_f32(i32 {})", y, a2);
            let z = self.tmp();
            let instr = match op {
                BinOp::Add => "fadd",
                BinOp::Sub => "fsub",
                BinOp::Mul => "fmul",
                BinOp::Div => "fdiv",
                _ => "fadd",
            };
            let _ = writeln!(self.buf, "  {} = {} float {}, {}", z, instr, x, y);
            let _ = writeln!(
                self.buf,
                "  call void @dream_store_f32(i32 {}, float {})",
                a3, z
            );
        }
    }
}
