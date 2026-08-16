//! String interners and leftover call/funcbox stubs.

use super::ModuleEmitter;
use super::names::*;
use dream_mir::{Callee, func_symbol};
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
