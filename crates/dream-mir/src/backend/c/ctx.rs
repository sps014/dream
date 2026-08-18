use crate::backend::c::native_layout::NativeLayouts;
use crate::backend::c::tables::{intern_strings, struct_tags, symbol_table};
use crate::{Mir, MirFunction};
use dream_hir::TypeLayout;
use dream_types::{DefId, TypeId, TypeInterner};
use indexmap::IndexMap;
use std::collections::HashMap;

pub(super) struct Cx<'a> {
    pub mir: &'a Mir,
    pub interner: &'a TypeInterner,
    pub strings: IndexMap<String, String>,
    pub symbols: HashMap<(DefId, Vec<TypeId>), String>,
    pub tags: HashMap<TypeId, i32>,
    pub ft: HashMap<(DefId, Vec<TypeId>), usize>,
    pub native: NativeLayouts,
    /// `--release`: skip per-index bounds traps so clang can vectorize counted loops.
    pub omit_bounds: bool,
}

impl<'a> Cx<'a> {
    pub(super) fn new_ex(mir: &'a Mir, interner: &'a TypeInterner, omit_bounds: bool) -> Self {
        let symbols = symbol_table(mir);
        let mut ft = HashMap::new();
        for (idx, f) in (1usize..).zip(mir.functions.iter()) {
            ft.insert((f.def, f.instance.clone()), idx);
        }
        Self {
            strings: intern_strings(mir, interner),
            symbols,
            tags: struct_tags(mir),
            ft,
            native: NativeLayouts::compute(mir, interner),
            mir,
            interner,
            omit_bounds,
        }
    }

    pub(super) fn nstruct(&self, ty: TypeId) -> Option<&TypeLayout> {
        self.native
            .structs
            .get(&ty)
            .or_else(|| self.mir.layouts.structs.get(&ty))
    }

    pub(super) fn nunion(&self, ty: TypeId) -> Option<&dream_hir::UnionLayout> {
        self.native
            .unions
            .get(&ty)
            .or_else(|| self.mir.layouts.unions.get(&ty))
    }

    pub(super) fn str_sym(&self, s: &str) -> &str {
        self.strings.get(s).unwrap_or_else(|| {
            crate::internal_error!("string literal {s:?} was not interned before C codegen")
        })
    }

    pub(super) fn type_tag(&self, ty: TypeId, fallback: DefId) -> i32 {
        self.tags.get(&ty).copied().unwrap_or(fallback.0 as i32)
    }

    pub(super) fn callee_c(&self, def: DefId, args: &[TypeId]) -> String {
        self.symbols
            .get(&(def, args.to_vec()))
            .cloned()
            .or_else(|| self.symbols.get(&(def, vec![])).cloned())
            .unwrap_or_else(|| format!("def{}", def.0))
    }

    pub(super) fn func_index(&self, f: &MirFunction) -> usize {
        *self
            .ft
            .get(&(f.def, f.instance.clone()))
            .unwrap_or(&0)
    }
}
