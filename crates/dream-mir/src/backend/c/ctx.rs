use crate::backend::c::native_layout::NativeLayouts;
use crate::backend::c::tables::{intern_strings, struct_tags, symbol_table};
use crate::backend::c::target::CTarget;
use crate::{Mir, MirFunction};
use dream_hir::TypeLayout;
use dream_types::{DefId, TypeId, TypeInterner};
use indexmap::IndexMap;
use std::collections::HashMap;

pub(super) struct Cx<'a> {
    pub mir: &'a Mir,
    pub interner: &'a TypeInterner,
    pub target: CTarget,
    pub strings: IndexMap<String, String>,
    pub symbols: HashMap<(DefId, Vec<TypeId>), String>,
    pub tags: HashMap<TypeId, i32>,
    pub ft: HashMap<(DefId, Vec<TypeId>), usize>,
    pub native: NativeLayouts,
    /// True when the module was compiled with debug info (`-g`): the analyzer only emits
    /// `Statement::DebugLine` markers then, so their presence is the backend's signal to add
    /// debugger-only views (async future-frame structs).
    pub debug_syms: bool,
}

impl<'a> Cx<'a> {
    pub(super) fn new(mir: &'a Mir, interner: &'a TypeInterner, target: CTarget) -> Self {
        let symbols = symbol_table(mir);
        let mut ft = HashMap::new();
        for (idx, f) in (1usize..).zip(mir.functions.iter()) {
            ft.insert((f.def, f.instance.clone()), idx);
        }
        let debug_syms = mir.functions.iter().any(|f| {
            f.blocks
                .iter()
                .any(|b| b.stmts.iter().any(|s| matches!(s, crate::Statement::DebugLine(_))))
        });
        Self {
            strings: intern_strings(mir, interner),
            symbols,
            tags: struct_tags(mir),
            ft,
            native: NativeLayouts::for_target(mir, interner, target),
            mir,
            interner,
            target,
            debug_syms,
        }
    }

    pub(super) fn nstruct(&self, ty: TypeId) -> Option<&TypeLayout> {
        self.native.structs.get(&ty)
    }

    pub(super) fn nunion(&self, ty: TypeId) -> Option<&dream_hir::UnionLayout> {
        self.native.unions.get(&ty)
    }

    pub(super) fn str_sym(&self, s: &str) -> &str {
        self.strings.get(s).unwrap_or_else(|| {
            crate::internal_error!("string literal {s:?} was not interned before C codegen")
        })
    }

    pub(super) fn type_tag(&self, ty: TypeId, _fallback: DefId) -> i32 {
        self.tags
            .get(&ty)
            .copied()
            .unwrap_or(crate::abi::TAG_STRUCT_BASE)
    }

    pub(super) fn callee_c(&self, def: DefId, args: &[TypeId]) -> String {
        self.symbols
            .get(&(def, args.to_vec()))
            .cloned()
            .or_else(|| self.symbols.get(&(def, vec![])).cloned())
            .unwrap_or_else(|| {
                crate::internal_error!("no C symbol for def{} instance {args:?}", def.0)
            })
    }

    pub(super) fn func_index(&self, f: &MirFunction) -> usize {
        *self.ft.get(&(f.def, f.instance.clone())).unwrap_or(&0)
    }
}
