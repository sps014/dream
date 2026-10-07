//! Cycle capability is computed from concrete ownership layouts, including inline values.
use dream_hir::LayoutTable;
use dream_types::{TyKind, TypeId, TypeInterner};
use std::collections::BTreeSet;

fn children(layouts: &LayoutTable, interner: &TypeInterner, ty: TypeId) -> Vec<TypeId> {
    if let Some(s) = layouts.structs.get(&ty) {
        return s
            .fields
            .iter()
            .filter(|f| !f.is_weak && !f.is_unowned)
            .map(|f| f.ty)
            .collect();
    }
    if let Some(u) = layouts.unions.get(&ty) {
        return u
            .variants
            .iter()
            .flat_map(|v| &v.fields)
            .filter(|f| !f.is_weak && !f.is_unowned)
            .map(|f| f.ty)
            .collect();
    }
    match interner.kind(ty) {
        TyKind::Array(e) => vec![*e],
        TyKind::Tuple(es) => es.clone(),
        _ => Vec::new(),
    }
}

pub(crate) fn cycle_capable(layouts: &LayoutTable, interner: &TypeInterner, root: TypeId) -> bool {
    if matches!(interner.kind(root), TyKind::Object | TyKind::Interface(..) | TyKind::Func(..)) { return true; }
    let mut pending = children(layouts, interner, root);
    let mut seen = BTreeSet::new();
    while let Some(ty) = pending.pop() {
        if ty == root {
            return true;
        }
        if !seen.insert(ty) {
            continue;
        }
        if matches!(
            interner.kind(ty),
            TyKind::Object | TyKind::Interface(..) | TyKind::Func(..)
        ) {
            return true;
        }
        if matches!(interner.kind(ty), TyKind::Struct(..))
            && !layouts.structs.contains_key(&ty)
            && interner.is_reference(ty)
        {
            return true;
        }
        pending.extend(children(layouts, interner, ty));
    }
    false
}

pub(crate) fn contains_cycle_refs(layouts: &LayoutTable, interner: &TypeInterner, root: TypeId) -> bool {
    if cycle_capable(layouts, interner, root) { return true; }
    let mut seen = BTreeSet::new();
    let mut pending = children(layouts, interner, root);
    while let Some(ty) = pending.pop() {
        if !seen.insert(ty) { continue; }
        if cycle_capable(layouts, interner, ty) { return true; }
        pending.extend(children(layouts, interner, ty));
    }
    false
}
