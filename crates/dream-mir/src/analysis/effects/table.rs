//! Whole-module closure over concrete functions, constructors and closed interface targets.

use super::intrinsics::{host_summary, intrinsic_summary};
use super::local::LocalSummary;
use super::{Known, ModRef, strong_children};
use crate::analysis::function_key::FunctionKeyRef;
use crate::{Callee, Global, Mir, MirFunction, Place, Rvalue, Statement};
use dream_abi::intrinsics::IntrinsicOp;
use dream_hir::LayoutTable;
use dream_types::{DefId, TyKind, TypeId, TypeInterner};
use indexmap::IndexMap;
use std::collections::{BTreeMap, BTreeSet};

/// Module-wide summaries. A lookup that misses (an empty table in unit tests, a pruned callee)
/// answers [`ModRef::Top`].
pub(crate) struct ModRefTable {
    pub(super) observers: IndexMap<(DefId, Vec<TypeId>), bool>,
    pub(super) cleanup: IndexMap<(DefId, Vec<TypeId>), bool>,
    pub(crate) del_observes: bool,
    pub(crate) release_proofs: bool,
    pub(super) fns: IndexMap<(DefId, Vec<TypeId>), ModRef>,
    globals: IndexMap<Global, TypeId>,
    /// Summary of `New { ctor }`: the constructor body minus its direct stores into the fresh
    /// `this` (no other reference to that object exists yet). Joined over instances.
    pub(super) ctors: IndexMap<DefId, ModRef>,
    /// Bodiless intrinsic defs whose runtime helper overwrites no Dream heap slot.
    intrinsics: IndexMap<DefId, ModRef>,
    rc_event_counters: BTreeSet<DefId>,
    capture_free_boxes: BTreeSet<TypeId>,
    /// Union of every `del` body: what may run inside any `Release` that frees an object.
    del: ModRef,
    /// `(iface id, method slot)` → join of the concrete methods. Missing slots are [`ModRef::Top`].
    pub(super) ifaces: IndexMap<(usize, usize), ModRef>,
    /// Resolved destructor-bearing types; `None` (an uncomputed table) means any type may.
    del_types: Option<BTreeSet<TypeId>>,
    interface_children: IndexMap<TypeId, Vec<TypeId>>,
}

impl Default for ModRefTable {
    fn default() -> Self {
        Self {
            observers: IndexMap::new(),
            cleanup: IndexMap::new(),
            del_observes: true,
            release_proofs: false,
            fns: IndexMap::new(),
            globals: IndexMap::new(),
            ctors: IndexMap::new(),
            intrinsics: IndexMap::new(),
            rc_event_counters: BTreeSet::new(),
            capture_free_boxes: BTreeSet::new(),
            del: ModRef::Top,
            ifaces: IndexMap::new(),
            del_types: None,
            interface_children: IndexMap::new(),
        }
    }
}

impl ModRefTable {
    pub(crate) fn compute(mir: &Mir, interner: &TypeInterner) -> Self {
        Self::build(mir, interner, true)
    }

    pub(crate) fn after_rc(mir: &Mir, interner: &TypeInterner) -> Self {
        Self::build(mir, interner, false)
    }

    fn build(mir: &Mir, interner: &TypeInterner, implicit_cleanup: bool) -> Self {
        let _effects =
            tracing::info_span!("heap_effects", functions = mir.functions.len()).entered();
        let intrinsics: IndexMap<DefId, ModRef> = mir
            .intrinsics
            .iter()
            .filter_map(|(def, key)| {
                if dream_abi::intrinsics::is_llvm_intrinsic_key(key) {
                    return Some((*def, ModRef::Known(Known::default())));
                }
                intrinsic_summary(IntrinsicOp::from_key(key)?).map(|s| (*def, s))
            })
            .chain(mir.imports.iter().filter_map(|import| {
                (!mir.profile.is_debug() && !import.is_async && import.params.is_empty())
                    .then(|| host_summary(&import.module, &import.field))
                    .flatten()
                    .map(|summary| (import.def, summary))
            }))
            .collect();
        let globals: IndexMap<Global, TypeId> = mir.globals.iter().map(|g| (g.id, g.ty)).collect();
        let mut table = ModRefTable {
            observers: IndexMap::new(),
            cleanup: IndexMap::new(),
            del_observes: false,
            release_proofs: !mir.profile.is_debug(),
            fns: IndexMap::new(),
            globals: globals.clone(),
            ctors: IndexMap::new(),
            intrinsics,
            rc_event_counters: mir
                .intrinsics
                .iter()
                .filter_map(|(def, key)| {
                    (IntrinsicOp::from_key(key) == Some(IntrinsicOp::DebugRuntimeCounter))
                        .then_some(*def)
                })
                .collect(),
            capture_free_boxes: if mir.profile.is_debug() {
                BTreeSet::new()
            } else {
                crate::analysis::function_boxes::capture_free_types(mir, interner)
            },
            del: ModRef::default(),
            ifaces: IndexMap::new(),
            interface_children: mir
                .interfaces
                .interfaces
                .iter()
                .enumerate()
                .map(|(id, info)| {
                    let children = mir
                        .interfaces
                        .impls
                        .iter()
                        .filter(|implementation| {
                            implementation
                                .entries
                                .iter()
                                .any(|(iface_id, _)| *iface_id == id)
                        })
                        .map(|implementation| implementation.class_ty)
                        .collect();
                    (info.ty, children)
                })
                .collect(),
            del_types: Some(
                mir.layouts
                    .structs
                    .iter()
                    .filter_map(|(ty, layout)| layout.has_destructor().then_some(*ty))
                    .chain(
                        mir.layouts
                            .unions
                            .iter()
                            .filter_map(|(ty, layout)| layout.has_destructor().then_some(*ty)),
                    )
                    .collect(),
            ),
        };
        let mut cleanup_types = BTreeMap::new();
        let locals: Vec<LocalSummary> = mir
            .functions
            .iter()
            .map(|f| {
                let may_destroy = f.locals.iter().enumerate().any(|(index, local)| {
                    let borrowed_parameter =
                        f.params.contains(&crate::Local(index as u32)) && !local.is_take;
                    (implicit_cleanup || (interner.is_value_type(local.ty) && !local.manual_drop))
                        && !local.is_ref
                        && !local.is_cursor
                        && !local.borrows_refs
                        && !borrowed_parameter
                        && *cleanup_types
                            .entry(local.ty)
                            .or_insert_with(|| table.may_run_del(local.ty, interner, &mir.layouts))
                });
                let may_destroy = may_destroy
                    || f.blocks
                        .iter()
                        .flat_map(|block| &block.stmts)
                        .any(|stmt| table.statement_may_destroy(f, stmt, interner, &mir.layouts));
                LocalSummary::of(
                    f,
                    interner,
                    &globals,
                    may_destroy,
                    table.release_proofs,
                    &table.rc_event_counters,
                )
            })
            .collect();
        for (f, s) in mir.functions.iter().zip(&locals) {
            table.fns.insert((f.def, f.instance.clone()), s.own.clone());
            let entry = table.ctors.entry(f.def).or_default();
            entry.join(&s.own_fresh);
        }
        table.close_effects(mir, &locals);
        let mut del = ModRef::default();
        let destructor_defs: BTreeSet<_> = mir
            .layouts
            .structs
            .values()
            .filter_map(|layout| layout.destructor)
            .chain(
                mir.layouts
                    .unions
                    .values()
                    .filter_map(|layout| layout.destructor),
            )
            .collect();
        let mut seen_destructors = BTreeSet::new();
        for f in &mir.functions {
            if destructor_defs.contains(&f.def) {
                seen_destructors.insert(f.def);
                del.join(&table.call_def(f.def, &f.instance));
                table.del_observes |= table.observes_def(f.def, &f.instance);
            }
        }
        for def in destructor_defs.difference(&seen_destructors) {
            del.join(&table.call_def(*def, &[]));
            table.del_observes |= table.observes_def(*def, &[]);
        }
        table.del = del;
        table
    }

    pub(crate) fn call_def(&self, def: DefId, args: &[TypeId]) -> ModRef {
        if let Some(s) = self.fns.get(&FunctionKeyRef { def, args }) {
            return s.clone();
        }
        self.intrinsics.get(&def).cloned().unwrap_or(ModRef::Top)
    }

    pub(crate) fn observes_def(&self, def: DefId, args: &[TypeId]) -> bool {
        self.observers
            .get(&FunctionKeyRef { def, args })
            .copied()
            .unwrap_or_else(|| match self.intrinsics.get(&def) {
                Some(ModRef::Known(k)) => k.observes_ownership(),
                _ => true,
            })
    }

    pub(crate) fn observes_ctor(&self, def: DefId) -> bool {
        self.observers
            .iter()
            .filter(|((candidate, _), _)| *candidate == def)
            .map(|(_, &observes)| observes)
            .reduce(|a, b| a || b)
            .unwrap_or(true)
    }

    pub(crate) fn ctor_may_destroy(&self, def: DefId) -> bool {
        self.cleanup
            .iter()
            .filter(|((candidate, _), _)| *candidate == def)
            .map(|(_, &destroys)| destroys)
            .reduce(|a, b| a || b)
            .unwrap_or(true)
    }

    pub(crate) fn call_may_destroy(&self, def: DefId, args: &[TypeId]) -> bool {
        self.cleanup
            .get(&FunctionKeyRef { def, args })
            .copied()
            .unwrap_or_else(|| self.call_def(def, args).may_destroy())
    }

    pub(crate) fn call(&self, callee: &Callee) -> ModRef {
        self.call_def(callee.def, &callee.args)
    }

    pub(crate) fn call_at(&self, callee: &Callee, arguments: &[crate::Operand]) -> ModRef {
        if self.release_proofs
            && Self::is_rc_event_counter(&self.rc_event_counters, callee.def, arguments)
        {
            ModRef::default()
        } else {
            self.call(callee)
        }
    }

    pub(crate) fn observes_call_at(&self, callee: &Callee, arguments: &[crate::Operand]) -> bool {
        !(self.release_proofs
            && Self::is_rc_event_counter(&self.rc_event_counters, callee.def, arguments))
            && self.observes_def(callee.def, &callee.args)
    }

    pub(super) fn is_rc_event_counter(
        defs: &BTreeSet<DefId>,
        def: DefId,
        arguments: &[crate::Operand],
    ) -> bool {
        // Event attribution measures the optimized code. Heap-byte queries and dynamic ids
        // can observe live storage, so only the retain/release event ids are exempt.
        defs.contains(&def)
            && matches!(
                arguments.first(),
                Some(crate::Operand::Const(crate::Const::Int(3 | 4)))
            )
    }

    pub(crate) fn ctor(&self, def: DefId) -> ModRef {
        self.ctors.get(&def).cloned().unwrap_or(ModRef::Top)
    }

    pub(crate) fn iface(&self, iface_id: usize, slot: usize) -> ModRef {
        self.ifaces
            .get(&(iface_id, slot))
            .cloned()
            .unwrap_or(ModRef::Top)
    }

    /// What any `del` may do; a `Release` anywhere may run one.
    pub(crate) fn del(&self) -> &ModRef {
        &self.del
    }

    /// Whether freeing a `ty` graph may run a `del` (walking strong fields; opaque types may
    /// hold anything).
    pub(crate) fn statement_may_destroy(
        &self,
        f: &MirFunction,
        stmt: &Statement,
        interner: &TypeInterner,
        layouts: &LayoutTable,
    ) -> bool {
        if let Statement::Assign(_, Rvalue::ArrayRealloc { elem_ty, .. }) = stmt
            && self.may_run_del(*elem_ty, interner, layouts)
        {
            return true;
        }
        let place_type = |place: &Place| match place {
            Place::Local(local) => Some(f.local_ty(*local)),
            Place::Global(global) => self.globals.get(global).copied(),
            Place::Field { base, field } => layouts
                .get(f.local_ty(*base))
                .and_then(|layout| layout.fields.get(*field))
                .filter(|field| !field.is_weak && !field.is_unowned)
                .map(|field| field.ty),
            Place::Index { base, .. } => match interner.kind(f.local_ty(*base)) {
                TyKind::Array(elem) => Some(*elem),
                _ => None,
            },
            Place::Deref { elem_ty, .. } => Some(*elem_ty),
        };
        let may_drop =
            |ty: Option<TypeId>| ty.is_none_or(|ty| self.may_run_del(ty, interner, layouts));
        match stmt {
            Statement::Assign(Place::Field { base, field }, _)
                if layouts
                    .get(f.local_ty(*base))
                    .and_then(|layout| layout.fields.get(*field))
                    .is_some_and(|field| field.is_weak || field.is_unowned) =>
            {
                false
            }
            Statement::Assign(
                place @ (Place::Field { .. } | Place::Index { .. } | Place::Global(_)),
                _,
            ) => may_drop(place_type(place)),
            Statement::Assign(_, Rvalue::ArrayRealloc { elem_ty, .. })
            | Statement::ArrayElemsCopy { elem_ty, .. }
            | Statement::ArrayElemsFill { elem_ty, .. } => may_drop(Some(*elem_ty)),
            Statement::Release(crate::Operand::Copy(place)) => may_drop(place_type(place)),
            Statement::ValueDrop(local) => may_drop(Some(f.local_ty(*local))),
            Statement::DeferLeave(_) => true,
            _ => false,
        }
    }

    pub(crate) fn may_run_del(
        &self,
        ty: TypeId,
        interner: &TypeInterner,
        layouts: &LayoutTable,
    ) -> bool {
        if matches!(
            interner.kind(ty),
            TyKind::Prim(_) | TyKind::Void | TyKind::Error | TyKind::Enum(_)
        ) {
            return false;
        }
        let Some(dels) = &self.del_types else {
            return true;
        };
        if dels.is_empty() {
            return false;
        }
        let mut seen = BTreeSet::new();
        let mut stack = vec![ty];
        while let Some(t) = stack.pop() {
            if !seen.insert(t) {
                continue;
            }
            if dels.contains(&t) {
                return true;
            }
            if self.capture_free_boxes.contains(&t) {
                continue;
            }
            if matches!(interner.kind(t), TyKind::Interface(..)) {
                match self.interface_children.get(&t) {
                    Some(children) if !children.is_empty() => {
                        stack.extend(children.iter().copied())
                    }
                    _ => return true,
                }
                continue;
            }
            match strong_children(t, interner, layouts) {
                Some(cs) => stack.extend(cs),
                None => return true,
            }
        }
        false
    }
}
