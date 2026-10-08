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
        TyKind::Union(_, args) if interner.is_niche_union(ty) => args.clone(),
        _ => Vec::new(),
    }
}

pub(crate) fn cycle_capable(layouts: &LayoutTable, interner: &TypeInterner, root: TypeId) -> bool {
    // Bulk region reclamation cannot skip user finalizers, even on acyclic layouts.
    if layouts
        .structs
        .get(&root)
        .is_some_and(|s| s.destructor.is_some())
        || matches!(
            interner.kind(root),
            TyKind::Object | TyKind::Interface(..) | TyKind::Func(..)
        )
        || (interner.is_reference(root)
            && match interner.kind(root) {
                TyKind::Struct(..) => !layouts.structs.contains_key(&root),
                TyKind::Union(..) => !layouts.unions.contains_key(&root),
                _ => false,
            })
    {
        return true;
    }
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
        if interner.is_reference(ty)
            && match interner.kind(ty) {
                TyKind::Struct(..) => !layouts.structs.contains_key(&ty),
                TyKind::Union(..) => !layouts.unions.contains_key(&ty),
                _ => false,
            }
        {
            return true;
        }
        pending.extend(children(layouts, interner, ty));
    }
    false
}
