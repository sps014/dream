//! Per-module codegen context: target, native layouts, symbol and
//! string tables, runtime type tags, function-table indices, and lazily built glue policy.

use super::glue::CanonMaps;
use super::iface_guard::GuardTable;
use super::native_layout::NativeLayouts;
use super::tables::{intern_strings, struct_tags, symbol_table};
use super::target::Target;
use crate::{Mir, MirFunction};
use dream_hir::TypeLayout;
use dream_types::{DefId, TyKind, TypeId, TypeInterner};
use indexmap::IndexMap;
use indexmap::IndexMap as HashMap;
use std::sync::OnceLock;

pub(crate) struct Cx<'a> {
    pub mir: &'a Mir,
    pub interner: &'a TypeInterner,
    pub target: Target,
    /// Literal → emitted symbol (`__ds{n}`), in first-use order.
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
    canon: OnceLock<CanonMaps>,
    iface_guards: OnceLock<GuardTable>,
    intrinsics: HashMap<DefId, String>,
}

impl<'a> Cx<'a> {
    pub(crate) fn new(mir: &'a Mir, interner: &'a TypeInterner, target: Target) -> Self {
        let symbols = symbol_table(mir);
        let mut ft = HashMap::new();
        for (idx, f) in (1usize..).zip(mir.functions.iter()) {
            ft.insert((f.def, f.instance.clone()), idx);
        }
        let debug_syms = mir.functions.iter().any(|f| {
            f.blocks.iter().any(|b| {
                b.stmts
                    .iter()
                    .any(|s| matches!(s, crate::Statement::DebugLine(_)))
            })
        });
        let mut intrinsics = HashMap::new();
        for (def, key) in &mir.intrinsics {
            intrinsics.entry(*def).or_insert_with(|| key.clone());
        }
        Self {
            strings: intern_strings(mir),
            symbols,
            tags: struct_tags(mir),
            ft,
            native: NativeLayouts::for_target(mir, interner, target),
            mir,
            interner,
            target,
            debug_syms,
            leak_checks: false,
            canon: OnceLock::new(),
            iface_guards: OnceLock::new(),
            intrinsics,
        }
    }

    pub(crate) fn with_leak_checks(
        mir: &'a Mir,
        interner: &'a TypeInterner,
        target: Target,
        leak_checks: bool,
    ) -> Self {
        let mut cx = Self::new(mir, interner, target);
        cx.leak_checks = leak_checks;
        cx
    }

    pub(crate) fn canon_maps(&self) -> &CanonMaps {
        self.canon.get_or_init(|| super::glue::canonical_maps(self))
    }

    pub(crate) fn iface_guard(&self, iface_id: usize, slot: usize) -> Option<&[(i32, String)]> {
        self.iface_guards
            .get_or_init(|| super::iface_guard::build_guards(self))
            .get(&(iface_id, slot))
            .map(Vec::as_slice)
    }

    pub(crate) fn nstruct(&self, ty: TypeId) -> Option<&TypeLayout> {
        self.native.structs.get(&ty)
    }

    pub(crate) fn nunion(&self, ty: TypeId) -> Option<&dream_hir::UnionLayout> {
        self.native.unions.get(&ty)
    }

    pub(crate) fn intrinsic_key(&self, def: DefId) -> Option<&str> {
        self.intrinsics.get(&def).map(String::as_str)
    }

    pub(crate) fn global_ty(&self, g: crate::Global) -> Option<TypeId> {
        self.mir
            .globals
            .iter()
            .find(|global| global.id == g)
            .map(|global| global.ty)
    }

    /// For a niche union, `(payload-variant discriminant, empty-variant discriminant)`. The
    /// classification guarantees exactly one variant of each shape.
    pub(crate) fn niche_variant_discriminants(&self, ty: TypeId) -> Option<(i32, i32)> {
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

    pub(crate) fn str_sym(&self, s: &str) -> &str {
        self.strings.get(s).unwrap_or_else(|| {
            crate::internal_error!("string literal {s:?} was not interned before codegen")
        })
    }

    pub(crate) fn type_tag(&self, ty: TypeId) -> i32 {
        self.tags
            .get(&ty)
            .copied()
            .unwrap_or(crate::abi::TAG_STRUCT_BASE)
    }

    /// The runtime tag an interface receiver of type `ty` carries, when it has one.
    pub(crate) fn interface_tag(&self, ty: TypeId) -> Option<i32> {
        match self.interner.kind(ty) {
            TyKind::Array(_) => Some(crate::abi::TAG_ARRAY),
            _ => self.tags.get(&ty).copied(),
        }
    }

    pub(crate) fn callee_sym(&self, def: DefId, args: &[TypeId]) -> String {
        self.symbols
            .get(&(def, args.to_vec()))
            .cloned()
            .or_else(|| self.symbols.get(&(def, vec![])).cloned())
            .unwrap_or_else(|| {
                crate::internal_error!("no symbol for def{} instance {args:?}", def.0)
            })
    }

    pub(crate) fn func_index(&self, f: &MirFunction) -> usize {
        *self.ft.get(&(f.def, f.instance.clone())).unwrap_or(&0)
    }

    /// Function-table layout: `[0]` reserved, `[1..=functions]` functions and async stubs, then
    /// one slot per coroutine poll, then one per deferred-extern poll.
    pub(crate) fn ftable_len(&self) -> usize {
        self.import_poll_base() + self.lazy_import_count()
    }

    pub(crate) fn import_poll_base(&self) -> usize {
        self.mir.functions.len() + 1 + self.mir.polls.len()
    }

    pub(crate) fn lazy_import_count(&self) -> usize {
        self.mir
            .imports
            .iter()
            .filter(|i| super::abi_types::lazy_import_poll(i).is_some())
            .count()
    }
}
