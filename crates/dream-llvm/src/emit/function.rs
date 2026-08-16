//! Function prologues, debug hooks, blocks.

use super::ModuleEmitter;
use super::names::*;
use dream_mir::func_symbol;
use dream_mir::{BlockId, MirFunction};
use dream_types::TyKind;
use std::fmt::Write as _;

impl<'a> ModuleEmitter<'a> {
    pub(crate) fn emit_function(&mut self, func: &MirFunction, fn_id: i32) {
        self.next = 0;
        self.cur_fn = fn_id;
        let name = resolved_symbol(&func_symbol(func));
        let ret_id = llvm_fn_ret(self.interner, &self.mir.layouts, func);
        let ret = llvm_val_ty(self.interner, ret_id);
        let mut args = Vec::new();
        for (i, p) in func.params.iter().enumerate() {
            args.push(format!("{} %a{}", llvm_val_ty(self.interner, func.local_ty(*p)), i));
        }
        let arglist = args.join(", ");
        let is_void = matches!(self.interner.kind(ret_id), TyKind::Void | TyKind::Error);
        if is_void {
            let _ = writeln!(self.buf, "\ndefine void @{}({}) {{", name, arglist);
        } else {
            let _ = writeln!(self.buf, "\ndefine {} @{}({}) {{", ret, name, arglist);
        }
        for (i, decl) in func.locals.iter().enumerate() {
            let ty = llvm_val_ty(self.interner, decl.ty);
            if ty != "void" {
                let _ = writeln!(self.buf, "  %l{} = alloca {}", i, ty);
            }
        }
        for (i, p) in func.params.iter().enumerate() {
            let ty = llvm_val_ty(self.interner, func.local_ty(*p));
            let _ = writeln!(self.buf, "  store {} %a{}, {}* %l{}", ty, i, ty, p.0);
        }
        if self.opts.debug_info && self.opts.triple.is_wasm() {
            let _ = writeln!(self.buf, "  call void @dream_debug_enter(i32 {})", fn_id);
        }
        let _ = writeln!(self.buf, "  br label %bb{}", func.entry.0);
        for (bi, _block) in func.blocks.iter().enumerate() {
            self.emit_block(func, BlockId(bi as u32));
        }
        self.buf.push_str("}\n");
    }

    pub(crate) fn emit_debug_exit(&mut self) {
        if self.opts.debug_info && self.opts.triple.is_wasm() {
            let _ = writeln!(
                self.buf,
                "  call void @dream_debug_exit(i32 {})",
                self.cur_fn
            );
        }
    }

    pub(crate) fn emit_debug_line(&mut self, func: &MirFunction, line: u32) {
        let file = debug_file_id(self.mir, func);
        let mut slot = 0u32;
        for (i, decl) in func.locals.iter().enumerate() {
            let Some(name) = decl.name.as_deref() else {
                continue;
            };
            if name.starts_with("__") {
                continue;
            }
            let lty = llvm_val_ty(self.interner, decl.ty);
            if lty == "void" {
                continue;
            }
            let loaded = self.tmp();
            let _ = writeln!(
                self.buf,
                "  {} = load {}, {}* %l{}",
                loaded, lty, lty, i
            );
            let bits = match lty {
                "i64" => loaded.clone(),
                "double" => {
                    let t = self.tmp();
                    let _ = writeln!(self.buf, "  {} = bitcast double {} to i64", t, loaded);
                    t
                }
                "float" => {
                    let t = self.tmp();
                    let _ = writeln!(self.buf, "  {} = bitcast float {} to i32", t, loaded);
                    let z = self.tmp();
                    let _ = writeln!(self.buf, "  {} = zext i32 {} to i64", z, t);
                    z
                }
                _ => {
                    let t = self.tmp();
                    let _ = writeln!(self.buf, "  {} = zext i32 {} to i64", t, loaded);
                    t
                }
            };
            let _ = writeln!(self.buf, "  store i64 {}, i64* @__dbg_v{}", bits, slot);
            slot += 1;
        }
        let _ = writeln!(
            self.buf,
            "  call void @dream_debug_line(i32 {}, i32 {})",
            file, line
        );
    }

    pub(crate) fn emit_block(&mut self, func: &MirFunction, id: BlockId) {
        let _ = writeln!(self.buf, "bb{}:", id.0);
        let block = func.block(id);
        for stmt in &block.stmts {
            self.emit_stmt(func, stmt);
        }
        self.emit_term(func, &block.terminator);
    }

}
