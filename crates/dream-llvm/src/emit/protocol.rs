//! Object protocol: drop, format, hash, array tags.

use super::ModuleEmitter;
use super::names::*;
use dream_hir::scalar_size;
use dream_mir::{Const, MirFunction, Operand, Place};
use dream_types::{PrimTy, TyKind, TypeId};
use std::fmt::Write as _;

impl<'a> ModuleEmitter<'a> {
    pub(crate) fn emit_default_protocol(&mut self) {
        let structs: Vec<(TypeId, dream_hir::TypeLayout)> = self
            .mir
            .layouts
            .structs
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        for (ty, layout) in &structs {
            if self.override_sym(*ty, "to_string").is_none() {
                self.emit_fmt_fn(*ty, layout);
            }
            if self.override_sym(*ty, "hash_code").is_none() {
                self.emit_hash_fn(*ty, layout);
            }
            self.emit_type_drop(*ty, layout);
        }
        let unions: Vec<(TypeId, dream_hir::UnionLayout)> = self
            .mir
            .layouts
            .unions
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        for (ty, layout) in &unions {
            self.emit_union_fmt(*ty, layout);
        }
        let array_elems: Vec<TypeId> = self.array_tags.keys().copied().collect();
        for elem in &array_elems {
            self.emit_array_fmt(*elem);
        }
        self.emit_object_dispatch(&structs);
        self.emit_array_drops();
        self.emit_drop_dispatch(&structs);
    }

    pub(crate) fn array_tag(&mut self, elem: TypeId) -> i32 {
        if let Some(t) = self.array_tags.get(&elem) {
            return *t;
        }
        let next = self
            .tags
            .values()
            .copied()
            .chain(self.array_tags.values().copied())
            .max()
            .unwrap_or(dream_mir::abi::TAG_STRUCT_BASE - 1)
            + 1;
        self.array_tags.insert(elem, next);
        next
    }

    pub(crate) fn emit_array_drops(&mut self) {
        let elems: Vec<(TypeId, i32)> = self.array_tags.iter().map(|(k, v)| (*k, *v)).collect();
        for (elem, _) in &elems {
            if !self.interner.is_reference(*elem) {
                continue;
            }
            let (es, _) = scalar_size(self.interner, *elem);
            self.next = 0;
            let _ = writeln!(
                self.buf,
                "\ndefine void @{}(i32 %p) {{",
                fmt_sym("adrop", *elem)
            );
            let len = self.tmp();
            let _ = writeln!(self.buf, "  {} = call i32 @dream_array_len(i32 %p)", len);
            let i = self.tmp();
            let _ = writeln!(self.buf, "  {} = alloca i32", i);
            let _ = writeln!(self.buf, "  store i32 0, i32* {}", i);
            self.buf.push_str("  br label %aloop\n");
            self.buf.push_str("aloop:\n");
            let iv = self.tmp();
            let _ = writeln!(self.buf, "  {} = load i32, i32* {}", iv, i);
            let cmp = self.tmp();
            let _ = writeln!(self.buf, "  {} = icmp slt i32 {}, {}", cmp, iv, len);
            let _ = writeln!(self.buf, "  br i1 {}, label %abody, label %adone", cmp);
            self.buf.push_str("abody:\n");
            let off = self.tmp();
            let _ = writeln!(self.buf, "  {} = mul i32 {}, {}", off, iv, es);
            let addr = self.tmp();
            let _ = writeln!(self.buf, "  {} = add i32 %p, {}", addr, off);
            let addr2 = self.tmp();
            let _ = writeln!(self.buf, "  {} = add i32 {}, 4", addr2, addr);
            let v = self.load_width(*elem, &addr2);
            let _ = writeln!(
                self.buf,
                "  call void @{}(i32 {})",
                release_sym(self.interner, *elem),
                v
            );
            let n = self.tmp();
            let _ = writeln!(self.buf, "  {} = add i32 {}, 1", n, iv);
            let _ = writeln!(self.buf, "  store i32 {}, i32* {}", n, i);
            self.buf.push_str("  br label %aloop\n");
            self.buf.push_str("adone:\n  ret void\n}\n");
        }
    }

    pub(crate) fn emit_type_drop(&mut self, ty: TypeId, layout: &dream_hir::TypeLayout) {
        self.next = 0;
        let _ = writeln!(self.buf, "\ndefine void @{}(i32 %p) {{", fmt_sym("drop", ty));
        if let Some(del) = self.override_sym(ty, "del") {
            let _ = writeln!(self.buf, "  call void @{}(i32 %p)", del);
        }
        for f in &layout.fields {
            if f.is_weak || f.is_unowned {
                continue;
            }
            if !self.interner.is_reference(f.ty) {
                continue;
            }
            let addr = self.tmp();
            let _ = writeln!(self.buf, "  {} = add i32 %p, {}", addr, f.offset);
            let v = self.load_width(f.ty, &addr);
            let _ = writeln!(
                self.buf,
                "  call void @{}(i32 {})",
                release_sym(self.interner, f.ty),
                v
            );
        }
        self.buf.push_str("  ret void\n}\n");
    }

    pub(crate) fn emit_drop_dispatch(&mut self, structs: &[(TypeId, dream_hir::TypeLayout)]) {
        self.next = 0;
        self.buf.push_str("\ndefine void @dream_drop(i32 %p) {\n");
        let z = self.tmp();
        let _ = writeln!(self.buf, "  {} = icmp eq i32 %p, 0", z);
        let _ = writeln!(self.buf, "  br i1 {}, label %drop_ret, label %drop_go", z);
        self.buf.push_str("drop_go:\n");
        let tag = self.tmp();
        let _ = writeln!(self.buf, "  {} = call i32 @dream_object_tag(i32 %p)", tag);
        let mut sw = String::new();
        let mut arms = Vec::new();
        for (ty, _) in structs {
            let t = self.tags.get(ty).copied().unwrap_or(0);
            let lab = format!("drp_{}", ty.0);
            let _ = write!(sw, " i32 {}, label %{}", t, lab);
            arms.push((lab, fmt_sym("drop", *ty)));
        }
        let arrs: Vec<(TypeId, i32)> = self.array_tags.iter().map(|(k, v)| (*k, *v)).collect();
        for (elem, t) in &arrs {
            if !self.interner.is_reference(*elem) {
                continue;
            }
            let lab = format!("adrp_{}", elem.0);
            let _ = write!(sw, " i32 {}, label %{}", t, lab);
            arms.push((lab, fmt_sym("adrop", *elem)));
        }
        let _ = writeln!(self.buf, "  switch i32 {}, label %drop_ret [{}]", tag, sw);
        for (lab, call) in arms {
            let _ = writeln!(self.buf, "{}:", lab);
            let _ = writeln!(self.buf, "  call void @{}(i32 %p)", call);
            self.buf.push_str("  br label %drop_ret\n");
        }
        self.buf.push_str("drop_ret:\n  ret void\n}\n");
    }

    pub(crate) fn emit_union_fmt(&mut self, ty: TypeId, layout: &dream_hir::UnionLayout) {
        self.next = 0;
        let _ = writeln!(self.buf, "\ndefine i32 @{}(i32 %p) {{", fmt_sym("ufmt", ty));
        let disc = self.tmp();
        let _ = writeln!(self.buf, "  {} = call i32 @dream_load_i32(i32 %p)", disc);
        let join = "uf_join";
        let defl = "uf_def";
        let mut sw = String::new();
        for (i, v) in layout.variants.iter().enumerate() {
            let _ = write!(sw, " i32 {}, label %ufv{}", v.discriminant, i);
        }
        let _ = writeln!(self.buf, "  switch i32 {}, label %{} [{}]", disc, defl, sw);
        let mut phi = Vec::new();
        for (i, var) in layout.variants.iter().enumerate() {
            let lab = format!("ufv{}", i);
            let _ = writeln!(self.buf, "{}:", lab);
            let s = if var.fields.is_empty() {
                self.intern_lit(&var.name)
            } else {
                let mut s = self.intern_lit(&format!("{}(", var.name));
                for (fi, f) in var.fields.iter().enumerate() {
                    if fi > 0 {
                        let comma = self.intern_lit(", ");
                        let n = self.tmp();
                        let _ = writeln!(
                            self.buf,
                            "  {} = call i32 @dream_concat_strings(i32 {}, i32 {})",
                            n, s, comma
                        );
                        s = n;
                    }
                    let labf = self.intern_lit(&format!("{}: ", f.name));
                    let n = self.tmp();
                    let _ = writeln!(
                        self.buf,
                        "  {} = call i32 @dream_concat_strings(i32 {}, i32 {})",
                        n, s, labf
                    );
                    s = n;
                    let addr = self.tmp();
                    let _ = writeln!(self.buf, "  {} = add i32 %p, {}", addr, f.offset);
                    let val = self.load_width(f.ty, &addr);
                    let fs = self.stringify_val(f.ty, &val);
                    let n = self.tmp();
                    let _ = writeln!(
                        self.buf,
                        "  {} = call i32 @dream_concat_strings(i32 {}, i32 {})",
                        n, s, fs
                    );
                    s = n;
                }
                let end = self.intern_lit(")");
                let n = self.tmp();
                let _ = writeln!(
                    self.buf,
                    "  {} = call i32 @dream_concat_strings(i32 {}, i32 {})",
                    n, s, end
                );
                n
            };
            phi.push((s, lab));
            let _ = writeln!(self.buf, "  br label %{}", join);
        }
        let _ = writeln!(self.buf, "{}:", defl);
        let empty = self.intern_lit("");
        phi.push((empty, defl.into()));
        let _ = writeln!(self.buf, "  br label %{}", join);
        let _ = writeln!(self.buf, "{}:", join);
        let r = self.tmp();
        let _ = writeln!(
            self.buf,
            "  {} = phi i32 {}\n  ret i32 {}\n}}\n",
            r,
            format_phi(&phi),
            r
        );
    }

    pub(crate) fn emit_array_fmt(&mut self, elem: TypeId) {
        self.next = 0;
        let (es, _) = scalar_size(self.interner, elem);
        let _ = writeln!(
            self.buf,
            "\ndefine i32 @{}(i32 %p) {{",
            fmt_sym("afmt", elem)
        );
        let sslot = self.tmp();
        let islot = self.tmp();
        let _ = writeln!(self.buf, "  {} = alloca i32", sslot);
        let _ = writeln!(self.buf, "  {} = alloca i32", islot);
        let open = self.intern_lit("[");
        let close = self.intern_lit("]");
        let comma = self.intern_lit(", ");
        let len = self.tmp();
        let _ = writeln!(self.buf, "  {} = call i32 @dream_array_len(i32 %p)", len);
        let _ = writeln!(self.buf, "  store i32 {}, i32* {}", open, sslot);
        let _ = writeln!(self.buf, "  store i32 0, i32* {}", islot);
        self.buf.push_str("  br label %af_loop\n");
        self.buf.push_str("af_loop:\n");
        let iv = self.tmp();
        let _ = writeln!(self.buf, "  {} = load i32, i32* {}", iv, islot);
        let cmp = self.tmp();
        let _ = writeln!(self.buf, "  {} = icmp slt i32 {}, {}", cmp, iv, len);
        let _ = writeln!(self.buf, "  br i1 {}, label %af_body, label %af_done", cmp);
        self.buf.push_str("af_body:\n");
        let s = self.tmp();
        let _ = writeln!(self.buf, "  {} = load i32, i32* {}", s, sslot);
        let nz = self.tmp();
        let _ = writeln!(self.buf, "  {} = icmp ne i32 {}, 0", nz, iv);
        let _ = writeln!(self.buf, "  br i1 {}, label %af_comma, label %af_elem", nz);
        self.buf.push_str("af_comma:\n");
        let sc = self.tmp();
        let _ = writeln!(
            self.buf,
            "  {} = call i32 @dream_concat_strings(i32 {}, i32 {})",
            sc, s, comma
        );
        let _ = writeln!(self.buf, "  store i32 {}, i32* {}", sc, sslot);
        self.buf.push_str("  br label %af_elem\n");
        self.buf.push_str("af_elem:\n");
        let s2 = self.tmp();
        let _ = writeln!(self.buf, "  {} = load i32, i32* {}", s2, sslot);
        let off = self.tmp();
        let _ = writeln!(self.buf, "  {} = mul i32 {}, {}", off, iv, es);
        let addr = self.tmp();
        let _ = writeln!(self.buf, "  {} = add i32 %p, {}", addr, off);
        let addr2 = self.tmp();
        let _ = writeln!(self.buf, "  {} = add i32 {}, 4", addr2, addr);
        let val = self.load_width(elem, &addr2);
        let fs = self.stringify_val(elem, &val);
        let n = self.tmp();
        let _ = writeln!(
            self.buf,
            "  {} = call i32 @dream_concat_strings(i32 {}, i32 {})",
            n, s2, fs
        );
        let _ = writeln!(self.buf, "  store i32 {}, i32* {}", n, sslot);
        let ni = self.tmp();
        let _ = writeln!(self.buf, "  {} = add i32 {}, 1", ni, iv);
        let _ = writeln!(self.buf, "  store i32 {}, i32* {}", ni, islot);
        self.buf.push_str("  br label %af_loop\n");
        self.buf.push_str("af_done:\n");
        let s3 = self.tmp();
        let _ = writeln!(self.buf, "  {} = load i32, i32* {}", s3, sslot);
        let out = self.tmp();
        let _ = writeln!(
            self.buf,
            "  {} = call i32 @dream_concat_strings(i32 {}, i32 {})",
            out, s3, close
        );
        let _ = writeln!(self.buf, "  ret i32 {}\n}}\n", out);
    }

    pub(crate) fn emit_fmt_fn(&mut self, ty: TypeId, layout: &dream_hir::TypeLayout) {
        self.next = 0;
        let _ = writeln!(self.buf, "\ndefine i32 @{}(i32 %p) {{", fmt_sym("fmt", ty));
        let mut s = self.intern_lit(&format!("{} {{ ", layout.name));
        for (i, f) in layout.fields.iter().enumerate() {
            if i > 0 {
                let comma = self.intern_lit(", ");
                let n = self.tmp();
                let _ = writeln!(
                    self.buf,
                    "  {} = call i32 @dream_concat_strings(i32 {}, i32 {})",
                    n, s, comma
                );
                s = n;
            }
            let lab = self.intern_lit(&format!("{}: ", f.name));
            let n = self.tmp();
            let _ = writeln!(
                self.buf,
                "  {} = call i32 @dream_concat_strings(i32 {}, i32 {})",
                n, s, lab
            );
            s = n;
            let addr = self.tmp();
            let _ = writeln!(self.buf, "  {} = add i32 %p, {}", addr, f.offset);
            let val = self.load_width(f.ty, &addr);
            let fs = self.stringify_val(f.ty, &val);
            let n = self.tmp();
            let _ = writeln!(
                self.buf,
                "  {} = call i32 @dream_concat_strings(i32 {}, i32 {})",
                n, s, fs
            );
            s = n;
        }
        let end = self.intern_lit(" }");
        let n = self.tmp();
        let _ = writeln!(
            self.buf,
            "  {} = call i32 @dream_concat_strings(i32 {}, i32 {})",
            n, s, end
        );
        let _ = writeln!(self.buf, "  ret i32 {}\n}}\n", n);
    }

    pub(crate) fn emit_hash_fn(&mut self, ty: TypeId, layout: &dream_hir::TypeLayout) {
        self.next = 0;
        let _ = writeln!(self.buf, "\ndefine i32 @{}(i32 %p) {{", fmt_sym("hash", ty));
        let mut h = "17".to_string();
        for f in &layout.fields {
            let addr = self.tmp();
            let _ = writeln!(self.buf, "  {} = add i32 %p, {}", addr, f.offset);
            let val = self.load_width(f.ty, &addr);
            let hv = self.hash_val(f.ty, &val);
            let m = self.tmp();
            let _ = writeln!(self.buf, "  {} = mul i32 {}, 31", m, h);
            let a = self.tmp();
            let _ = writeln!(self.buf, "  {} = add i32 {}, {}", a, m, hv);
            h = a;
        }
        let _ = writeln!(self.buf, "  ret i32 {}\n}}\n", h);
    }

    pub(crate) fn hash_val(&mut self, ty: TypeId, v: &str) -> String {
        match self.interner.kind(ty) {
            TyKind::Prim(PrimTy::String) => {
                let t = self.tmp();
                let _ = writeln!(self.buf, "  {} = call i32 @dream_hash_bytes(i32 {})", t, v);
                t
            }
            TyKind::Struct(..) => {
                let t = self.tmp();
                let name = self
                    .override_sym(ty, "hash_code")
                    .unwrap_or_else(|| fmt_sym("hash", ty));
                let _ = writeln!(self.buf, "  {} = call i32 @{}(i32 {})", t, name, v);
                t
            }
            TyKind::Prim(PrimTy::Long | PrimTy::ULong) => {
                let t = self.tmp();
                let _ = writeln!(self.buf, "  {} = trunc i64 {} to i32", t, v);
                t
            }
            TyKind::Prim(PrimTy::Float) => {
                let t = self.tmp();
                let _ = writeln!(self.buf, "  {} = bitcast float {} to i32", t, v);
                t
            }
            TyKind::Prim(PrimTy::Double) => {
                let b = self.tmp();
                let t = self.tmp();
                let _ = writeln!(self.buf, "  {} = bitcast double {} to i64", b, v);
                let _ = writeln!(self.buf, "  {} = trunc i64 {} to i32", t, b);
                t
            }
            _ => v.to_string(),
        }
    }

    pub(crate) fn emit_object_dispatch(&mut self, structs: &[(TypeId, dream_hir::TypeLayout)]) {
        self.next = 0;
        self.buf.push_str("\ndefine i32 @dream_object_to_string(i32 %p) {\n");
        let empty = self.intern_lit("");
        let tag = self.tmp();
        let _ = writeln!(self.buf, "  {} = call i32 @dream_object_tag(i32 %p)", tag);
        let defl = "ots_def";
        let join = "ots_join";
        let mut sw = String::new();
        let mut arms: Vec<(i32, String, String)> = Vec::new();
        arms.push((dream_mir::abi::TAG_INT, "ots_int".into(), String::new()));
        arms.push((dream_mir::abi::TAG_STRING, "ots_str".into(), String::new()));
        arms.push((dream_mir::abi::TAG_BOOL, "ots_bool".into(), String::new()));
        for (ty, _) in structs {
            let t = self.tags.get(ty).copied().unwrap_or(0);
            let lab = format!("ots_{}", ty.0);
            let call = self
                .override_sym(*ty, "to_string")
                .unwrap_or_else(|| fmt_sym("fmt", *ty));
            arms.push((t, lab, call));
        }
        for ty in self.mir.layouts.unions.keys() {
            let t = self.tags.get(ty).copied().unwrap_or(0);
            arms.push((t, format!("otsu_{}", ty.0), fmt_sym("ufmt", *ty)));
        }
        let arr_tags: Vec<(TypeId, i32)> = self.array_tags.iter().map(|(k, v)| (*k, *v)).collect();
        for (elem, t) in arr_tags {
            arms.push((t, format!("otsa_{}", elem.0), fmt_sym("afmt", elem)));
        }
        for (tg, lab, _) in &arms {
            let _ = write!(sw, " i32 {}, label %{}", tg, lab);
        }
        let _ = writeln!(self.buf, "  switch i32 {}, label %{} [{}]", tag, defl, sw);
        let mut phi = Vec::new();
        let _ = writeln!(self.buf, "ots_int:");
        let u = self.tmp();
        let s = self.tmp();
        let _ = writeln!(self.buf, "  {} = call i32 @dream_unbox_i32(i32 %p)", u);
        let _ = writeln!(self.buf, "  {} = call i32 @dream_i32_to_string(i32 {})", s, u);
        phi.push((s.clone(), "ots_int".into()));
        let _ = writeln!(self.buf, "  br label %{}", join);
        let _ = writeln!(self.buf, "ots_str:");
        phi.push(("%p".into(), "ots_str".into()));
        let _ = writeln!(self.buf, "  br label %{}", join);
        let _ = writeln!(self.buf, "ots_bool:");
        let ub = self.tmp();
        let sb = self.tmp();
        let _ = writeln!(self.buf, "  {} = call i32 @dream_unbox_i32(i32 %p)", ub);
        let _ = writeln!(
            self.buf,
            "  {} = call i32 @dream_bool_to_string(i32 {})",
            sb, ub
        );
        phi.push((sb, "ots_bool".into()));
        let _ = writeln!(self.buf, "  br label %{}", join);
        for (tg, lab, call) in &arms {
            if *tg == dream_mir::abi::TAG_INT
                || *tg == dream_mir::abi::TAG_STRING
                || *tg == dream_mir::abi::TAG_BOOL
            {
                continue;
            }
            let _ = writeln!(self.buf, "{}:", lab);
            let r = self.tmp();
            let _ = writeln!(self.buf, "  {} = call i32 @{}(i32 %p)", r, call);
            phi.push((r, lab.clone()));
            let _ = writeln!(self.buf, "  br label %{}", join);
        }
        let _ = writeln!(self.buf, "{}:", defl);
        let _ = writeln!(self.buf, "  br label %{}", join);
        phi.push((empty, defl.into()));
        let _ = writeln!(self.buf, "{}:", join);
        let r = self.tmp();
        let _ = writeln!(
            self.buf,
            "  {} = phi i32 {}\n  ret i32 {}\n}}\n",
            r,
            format_phi(&phi),
            r
        );

        self.next = 0;
        self.buf.push_str("\ndefine i32 @dream_object_hash(i32 %p) {\n");
        let tag = self.tmp();
        let _ = writeln!(self.buf, "  {} = call i32 @dream_object_tag(i32 %p)", tag);
        let defl = "oh_def";
        let join = "oh_join";
        let mut sw = String::new();
        let mut arms: Vec<(i32, String, String)> = Vec::new();
        for (ty, _) in structs {
            let t = self.tags.get(ty).copied().unwrap_or(0);
            let lab = format!("oh_{}", ty.0);
            let call = self
                .override_sym(*ty, "hash_code")
                .unwrap_or_else(|| fmt_sym("hash", *ty));
            arms.push((t, lab, call));
        }
        for (tg, lab, _) in &arms {
            let _ = write!(sw, " i32 {}, label %{}", tg, lab);
        }
        let _ = writeln!(self.buf, "  switch i32 {}, label %{} [{}]", tag, defl, sw);
        let mut phi = Vec::new();
        for (_, lab, call) in &arms {
            let _ = writeln!(self.buf, "{}:", lab);
            let r = self.tmp();
            let _ = writeln!(self.buf, "  {} = call i32 @{}(i32 %p)", r, call);
            phi.push((r, lab.clone()));
            let _ = writeln!(self.buf, "  br label %{}", join);
        }
        let _ = writeln!(self.buf, "{}:", defl);
        let _ = writeln!(self.buf, "  br label %{}", join);
        phi.push(("0".into(), defl.into()));
        let _ = writeln!(self.buf, "{}:", join);
        let r = self.tmp();
        let _ = writeln!(
            self.buf,
            "  {} = phi i32 {}\n  ret i32 {}\n}}\n",
            r,
            format_phi(&phi),
            r
        );
    }

    pub(crate) fn op_ty(&self, func: &MirFunction, op: &Operand) -> TypeId {
        match op {
            Operand::Copy(p) => self.pl_ty(func, p),
            Operand::Const(Const::Long(_)) => self.interner.long(),
            Operand::Const(Const::Float(_)) => self.interner.double(),
            Operand::Const(Const::F32(_)) => self.interner.float(),
            Operand::Const(Const::Str(_)) => self.interner.string(),
            Operand::Const(Const::Bool(_)) => self.interner.bool(),
            _ => self.interner.int(),
        }
    }

    pub(crate) fn pl_ty(&self, func: &MirFunction, p: &Place) -> TypeId {
        match p {
            Place::Local(l) => func.local_ty(*l),
            Place::Global(g) => self
                .mir
                .globals
                .get(g.0 as usize)
                .map(|d| d.ty)
                .unwrap_or_else(|| self.interner.int()),
            Place::Field { base, field } => self
                .mir
                .layouts
                .get(func.local_ty(*base))
                .and_then(|l| l.fields.get(*field))
                .map(|f| f.ty)
                .unwrap_or_else(|| self.interner.int()),
            Place::Index { base, .. } => match self.interner.kind(func.local_ty(*base)) {
                TyKind::Array(e) => *e,
                _ => self.interner.int(),
            },
            Place::Deref { elem_ty, .. } => *elem_ty,
        }
    }
}
