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
    /// Emit the exit-time leak report from native `main` unconditionally (debug builds).
    pub leak_checks: bool,
    /// Lazily-computed release/destroy symbol canonicalization (types whose ARC glue
    /// bodies are byte-identical share one emitted function). See `release::canonical_maps`.
    pub canon: std::sync::OnceLock<super::release::CanonMaps>,
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
            leak_checks: false,
            canon: std::sync::OnceLock::new(),
        }
    }

    pub(super) fn with_leak_checks(
        mir: &'a Mir,
        interner: &'a TypeInterner,
        target: CTarget,
        leak_checks: bool,
    ) -> Self {
        let mut cx = Self::new(mir, interner, target);
        cx.leak_checks = leak_checks;
        cx
    }

    pub(super) fn canon_maps(&self) -> &super::release::CanonMaps {
        self.canon.get_or_init(|| super::release::canonical_maps(self))
    }

    pub(super) fn nstruct(&self, ty: TypeId) -> Option<&TypeLayout> {
        self.native.structs.get(&ty)
    }

    pub(super) fn nunion(&self, ty: TypeId) -> Option<&dream_hir::UnionLayout> {
        self.native.unions.get(&ty)
    }

    /// For a niche union, `(payload-variant discriminant, empty-variant discriminant)`. The
    /// classification guarantees exactly one variant of each shape.
    pub(super) fn niche_variant_discriminants(&self, ty: TypeId) -> Option<(i32, i32)> {
        if !self.interner.is_niche_union(ty) {
            return None;
        }
        let u = self.nunion(ty)?;
        let (some, none) = u
            .variants
            .iter()
            .partition::<Vec<_>, _>(|v| !v.fields.is_empty());
        match (some.first(), none.first()) {
            (Some(s), Some(n)) => Some((s.discriminant, n.discriminant)),
            _ => None,
        }
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
