//! Textual LLVM IR from MIR. Locals are allocas; heap traffic goes through `dream-rt`.

mod names;
mod function;
mod stmt;
mod ops;
mod place;
mod call;
mod intern;
mod protocol;

pub(crate) use names::*;

use crate::options::CodegenOptions;
use dream_mir::{func_symbol, struct_tags};
use dream_mir::Mir;
use dream_types::{TyKind, TypeId, TypeInterner};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

pub fn emit_module_ir(mir: &Mir, interner: &TypeInterner, opts: &CodegenOptions) -> String {
    let e = ModuleEmitter {
        mir,
        interner,
        opts,
        tags: struct_tags(mir),
        array_tags: BTreeMap::new(),
        buf: String::new(),
        globals: String::new(),
        next: 0,
        str_id: 0,
        stubs: Vec::new(),
        cur_fn: 0,
    };
    e.emit()
}

pub(crate) struct ModuleEmitter<'a> {
    pub(crate) mir: &'a Mir,
    pub(crate) interner: &'a TypeInterner,
    pub(crate) opts: &'a CodegenOptions,
    pub(crate) tags: HashMap<TypeId, i32>,
    pub(crate) array_tags: BTreeMap<TypeId, i32>,
    pub(crate) buf: String,
    pub(crate) globals: String,
    pub(crate) next: u32,
    pub(crate) str_id: u32,
    pub(crate) stubs: Vec<(String, String, Vec<String>)>,
    pub(crate) cur_fn: i32,
}

impl<'a> ModuleEmitter<'a> {
    pub(crate) fn emit(mut self) -> String {
        let _ = writeln!(self.buf, "; ModuleID = 'dream'");
        let _ = writeln!(self.buf, "target triple = \"{}\"", self.opts.triple.as_str());
        self.buf.push_str(RUNTIME_DECLS);
        self.emit_import_stubs();
        self.seed_array_tags();
        for g in &self.mir.globals {
            let ty = llvm_val_ty(self.interner, g.ty);
            let _ = writeln!(self.buf, "@g{} = global {} {}", g.id.0, ty, zero(ty));
        }
        if self.opts.debug_info {
            if self.opts.triple.is_wasm() {
                self.buf.push_str(DEBUG_DECLS_WASM);
                self.buf.push_str(DEBUG_ATTRS);
            } else {
                self.buf.push_str(DEBUG_DECLS);
            }
            let mut slots = 0u32;
            for f in &self.mir.functions {
                slots = slots.max(dbg_spill_count(f, self.interner));
            }
            for i in 0..slots {
                let _ = writeln!(self.globals, "@__dbg_v{} = global i64 0", i);
            }
        }
        for (i, f) in self.mir.functions.iter().enumerate() {
            if native_c_sym(&func_symbol(f)).is_some() || is_c_runtime_sym(&func_symbol(f)) {
                continue;
            }
            self.emit_function(f, i as i32);
        }
        self.emit_default_protocol();
        self.emit_sleep_stub();
        self.emit_funcbox_stubs();
        self.emit_worker_invoke();
        self.emit_call_stubs();
        if let Some(main) = self.mir.functions.iter().find(|f| f.name == "main") {
            let sym = llvm_fn_name(&func_symbol(main));
            let ret_ty = llvm_fn_ret(self.interner, &self.mir.layouts, main);
            self.buf.push_str("\ndefine void @dream_user_main() {\n");
            if matches!(self.interner.kind(ret_ty), TyKind::Void | TyKind::Error) {
                let _ = writeln!(self.buf, "  call void @{}()", sym);
            } else {
                let ty = llvm_val_ty(self.interner, ret_ty);
                let _ = writeln!(self.buf, "  call {} @{}()", ty, sym);
            }
            self.buf.push_str("  ret void\n}\n");
        }
        let mut out = String::new();
        let split = self.buf.find(RUNTIME_DECLS).unwrap_or(0) + RUNTIME_DECLS.len();
        out.push_str(&self.buf[..split]);
        out.push_str(&self.globals);
        out.push_str(&self.buf[split..]);
        out
    }

    pub(crate) fn seed_array_tags(&mut self) {
        let elems: Vec<TypeId> = self
            .mir
            .layouts
            .structs
            .values()
            .flat_map(|l| l.fields.iter())
            .filter_map(|f| match self.interner.kind(f.ty) {
                TyKind::Array(e) => Some(*e),
                _ => None,
            })
            .collect();
        for e in elems {
            let _ = self.array_tag(e);
        }
    }

    pub(crate) fn tmp(&mut self) -> String {
        let n = self.next;
        self.next += 1;
        format!("%t{}", n)
    }
}

