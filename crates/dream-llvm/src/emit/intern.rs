//! String interners and leftover call/funcbox stubs.

use super::ModuleEmitter;
use super::names::*;
use dream_mir::{Callee, Statement, func_symbol};
use dream_types::TyKind;
use std::collections::BTreeSet;
use std::fmt::Write as _;

impl<'a> ModuleEmitter<'a> {
    pub(crate) fn intern_cstr(&mut self, s: &str) -> String {
        let id = self.str_id;
        self.str_id += 1;
        let n = s.len() + 1;
        let mut esc = String::new();
        for b in s.bytes() {
            match b {
                b'\\' => esc.push_str("\\\\"),
                b'"' => esc.push_str("\\22"),
                32..=126 => esc.push(b as char),
                _ => {
                    let _ = write!(esc, "\\{:02X}", b);
                }
            }
        }
        let _ = writeln!(
            self.globals,
            "@.s{} = private constant [{} x i8] c\"{}\\00\"",
            id, n, esc
        );
        format!(
            "getelementptr inbounds ([{} x i8], [{} x i8]* @.s{}, i32 0, i32 0)",
            n, n, id
        )
    }

    pub(crate) fn intern_lit(&mut self, s: &str) -> String {
        self.intern_bytes(s.as_bytes())
    }

    pub(crate) fn intern_bytes(&mut self, bytes: &[u8]) -> String {
        let id = self.str_id;
        self.str_id += 1;
        let n = bytes.len() + 1;
        let mut esc = String::new();
        for &b in bytes {
            match b {
                b'\\' => esc.push_str("\\\\"),
                b'"' => esc.push_str("\\22"),
                32..=126 => esc.push(b as char),
                _ => {
                    let _ = write!(esc, "\\{:02X}", b);
                }
            }
        }
        let _ = writeln!(
            self.globals,
            "@.s{} = private constant [{} x i8] c\"{}\\00\"",
            id, n, esc
        );
        let r = self.tmp();
        let _ = writeln!(
            self.buf,
            "  {} = call i32 @dream_intern_utf8(i8* getelementptr inbounds ([{} x i8], [{} x i8]* @.s{}, i32 0, i32 0), i32 {})",
            r,
            n,
            n,
            id,
            bytes.len()
        );
        r
    }

    pub(crate) fn resolve_callee(&mut self, callee: &Callee, ret: &str, arg_tys: &[String]) -> String {
        for f in &self.mir.functions {
            if f.def == callee.def && f.instance == callee.args {
                return resolved_symbol(&func_symbol(f));
            }
        }
        for f in &self.mir.functions {
            if f.def == callee.def {
                return resolved_symbol(&func_symbol(f));
            }
        }
        for (def, key) in &self.mir.intrinsics {
            if *def == callee.def {
                return llvm_extern_name(key);
            }
        }
        for imp in &self.mir.imports {
            if imp.def == callee.def {
                if let Some(c) = native_c_sym(&imp.field).or_else(|| native_c_sym(&imp.name)) {
                    return c.to_string();
                }
                return llvm_extern_name(&imp.name);
            }
        }
        let name = llvm_fn_name(&format!("def{}", callee.def.0));
        if !self.stubs.iter().any(|(n, _, _)| n == &name) {
            self.stubs
                .push((name.clone(), ret.to_string(), arg_tys.to_vec()));
        }
        name
    }

    pub(crate) fn emit_sleep_stub(&mut self) {
        self.buf.push_str("\ndefine i32 @d_sleep(i32 %ms) {\n  ret i32 0\n}\n");
    }

    fn funcref_indices(&self) -> BTreeSet<usize> {
        let mut ids = BTreeSet::new();
        for f in &self.mir.functions {
            for b in &f.blocks {
                for st in &b.stmts {
                    if let Statement::Assign(_, dream_mir::Rvalue::FuncRef(c)) = st {
                        if let Some(i) = self
                            .mir
                            .functions
                            .iter()
                            .position(|g| g.def == c.def && g.instance == c.args)
                            .or_else(|| {
                                self.mir.functions.iter().position(|g| g.def == c.def)
                            })
                        {
                            ids.insert(i);
                        }
                    }
                }
            }
        }
        ids
    }

    pub(crate) fn emit_worker_invoke(&mut self) {
        self.buf.push_str(
            "\ndefine i32 @__dream_worker_invoke(i32 %idx, i32 %env, i32 %msg) {\n",
        );
        // Capturing bodies read `$__closure_env` (always MIR global 0), not an extra parameter.
        if !self.mir.globals.is_empty() {
            self.buf.push_str("  store i32 %env, i32* @g0\n");
        }
        let refs = self.funcref_indices();
        let mut sw = String::new();
        let mut arms = Vec::new();
        for (i, f) in self.mir.functions.iter().enumerate() {
            if !refs.contains(&i) {
                continue;
            }
            if f.params.len() != 1 {
                continue;
            }
            if llvm_val_ty(self.interner, f.local_ty(f.params[0])) != "i32" {
                continue;
            }
            let ret = llvm_fn_ret(self.interner, &self.mir.layouts, f);
            let rty = llvm_val_ty(self.interner, ret);
            if rty != "i32" && !matches!(self.interner.kind(ret), TyKind::Void | TyKind::Error) {
                continue;
            }
            let lab = format!("wki{}", i);
            let _ = write!(sw, " i32 {}, label %{}", i, lab);
            arms.push((i, lab, rty == "i32"));
        }
        let _ = writeln!(
            self.buf,
            "  switch i32 %idx, label %wmiss [{}]",
            sw
        );
        for (i, lab, has_ret) in arms {
            let _ = writeln!(self.buf, "{}:", lab);
            let f = &self.mir.functions[i];
            let name = resolved_symbol(&func_symbol(f));
            if has_ret {
                let t = format!("%wr{}", i);
                let _ = writeln!(self.buf, "  {} = call i32 @{}(i32 %msg)", t, name);
                let _ = writeln!(self.buf, "  ret i32 {}", t);
            } else {
                let _ = writeln!(self.buf, "  call void @{}(i32 %msg)", name);
                self.buf.push_str("  ret i32 0\n");
            }
        }
        self.buf.push_str("wmiss:\n  ret i32 0\n}\n");
        self.emit_guest_call();
    }

    pub(crate) fn emit_guest_call(&mut self) {
        self.buf.push_str(
            "\ndefine i64 @dream_call_guest(i32 %idx, i64 %a0, i64 %a1, i64 %a2, i64 %a3, i64 %a4, i64 %a5, i64 %a6, i64 %a7) {\n",
        );
        let refs = self.funcref_indices();
        let mut sw = String::new();
        let mut arms = Vec::new();
        for (i, f) in self.mir.functions.iter().enumerate() {
            if !refs.contains(&i) || f.params.len() > 8 {
                continue;
            }
            let lab = format!("gci{}", i);
            let _ = write!(sw, " i32 {}, label %{}", i, lab);
            arms.push((i, lab));
        }
        let _ = writeln!(self.buf, "  switch i32 %idx, label %gcmiss [{}]", sw);
        for (i, lab) in arms {
            let _ = writeln!(self.buf, "{}:", lab);
            let f = &self.mir.functions[i];
            let name = resolved_symbol(&func_symbol(f));
            let mut args = Vec::new();
            for (pi, p) in f.params.iter().enumerate() {
                let lty = llvm_val_ty(self.interner, f.local_ty(*p));
                let src = format!("%a{}", pi);
                match lty {
                    "i32" => {
                        let t = format!("%gci{}_{}", i, pi);
                        let _ = writeln!(self.buf, "  {} = trunc i64 {} to i32", t, src);
                        args.push(format!("i32 {}", t));
                    }
                    "float" => {
                        let t = format!("%gci{}_{}t", i, pi);
                        let f32v = format!("%gci{}_{}f", i, pi);
                        let _ = writeln!(self.buf, "  {} = trunc i64 {} to i32", t, src);
                        let _ = writeln!(self.buf, "  {} = bitcast i32 {} to float", f32v, t);
                        args.push(format!("float {}", f32v));
                    }
                    "double" => {
                        let d = format!("%gci{}_{}d", i, pi);
                        let _ = writeln!(self.buf, "  {} = bitcast i64 {} to double", d, src);
                        args.push(format!("double {}", d));
                    }
                    _ => args.push(format!("i64 {}", src)),
                }
            }
            let ret = llvm_fn_ret(self.interner, &self.mir.layouts, f);
            let rty = llvm_val_ty(self.interner, ret);
            if matches!(self.interner.kind(ret), TyKind::Void | TyKind::Error) {
                let _ = writeln!(self.buf, "  call void @{}({})", name, args.join(", "));
                self.buf.push_str("  ret i64 0\n");
            } else {
                let rv = format!("%gcr{}", i);
                let _ = writeln!(
                    self.buf,
                    "  {} = call {} @{}({})",
                    rv,
                    rty,
                    name,
                    args.join(", ")
                );
                match rty {
                    "i32" => {
                        let z = format!("%gcz{}", i);
                        let _ = writeln!(self.buf, "  {} = zext i32 {} to i64", z, rv);
                        let _ = writeln!(self.buf, "  ret i64 {}", z);
                    }
                    "float" => {
                        let t = format!("%gct{}", i);
                        let z = format!("%gcz{}", i);
                        let _ = writeln!(self.buf, "  {} = bitcast float {} to i32", t, rv);
                        let _ = writeln!(self.buf, "  {} = zext i32 {} to i64", z, t);
                        let _ = writeln!(self.buf, "  ret i64 {}", z);
                    }
                    "double" => {
                        let d = format!("%gcd{}", i);
                        let _ = writeln!(self.buf, "  {} = bitcast double {} to i64", d, rv);
                        let _ = writeln!(self.buf, "  ret i64 {}", d);
                    }
                    _ => {
                        let _ = writeln!(self.buf, "  ret i64 {}", rv);
                    }
                }
            }
        }
        self.buf.push_str("gcmiss:\n  ret i64 0\n}\n");
    }

    pub(crate) fn emit_funcbox_stubs(&mut self) {
        self.buf.push_str(
            r#"
define i32 @d_funcbox_env(i32 %p) {
  %a = add i32 %p, 4
  %r = call i32 @dream_load_i32(i32 %a)
  ret i32 %r
}
define i32 @d_funcbox_funcidx(i32 %p) {
  %r = call i32 @dream_load_i32(i32 %p)
  ret i32 %r
}
define void @d_js_retain(i32 %p) {
  ret void
}
define void @d_js_release(i32 %p) {
  ret void
}
define i32 @d_funcbox_new(i32 %idx, i32 %env) {
  %p = call i32 @dream_malloc(i32 8, i32 12)
  call void @dream_store_i32(i32 %p, i32 %idx)
  %a = add i32 %p, 4
  call void @dream_store_i32(i32 %a, i32 %env)
  call void @dream_retain(i32 %env)
  ret i32 %p
}
"#,
        );
    }

    pub(crate) fn emit_call_stubs(&mut self) {
        let stubs = self.stubs.clone();
        for (name, ret, args) in stubs {
            if name == "d_sleep"
                || name.starts_with("d_funcbox_")
                || name.starts_with("d_js_")
                || name.starts_with("dream_")
                || name == "__dream_worker_invoke"
                || name == "dream_call_guest"
            {
                continue;
            }
            if self
                .mir
                .functions
                .iter()
                .any(|f| llvm_fn_name(&func_symbol(f)) == name)
            {
                continue;
            }
            let params: Vec<String> = args
                .iter()
                .enumerate()
                .map(|(i, t)| format!("{} %a{}", t, i))
                .collect();
            if ret == "void" {
                let _ = writeln!(
                    self.buf,
                    "\ndefine void @{}({}) {{\n  ret void\n}}\n",
                    name,
                    params.join(", ")
                );
            } else {
                let _ = writeln!(
                    self.buf,
                    "\ndefine {} @{}({}) {{\n  ret {} {}\n}}\n",
                    ret,
                    name,
                    params.join(", "),
                    ret,
                    zero(&ret)
                );
            }
        }
    }

}
