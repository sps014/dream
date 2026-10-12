//! Strong ownership edges used by cleanup proofs.

use dream_hir::LayoutTable;
use dream_types::{TyKind, TypeId, TypeInterner};

/// Types directly owned by a `ty` value through strong slots, or `None` if its contents are
/// unknown (`object`, interfaces, `fun`, `js`, missing layouts).
pub(crate) fn strong_children(
    ty: TypeId,
    interner: &TypeInterner,
    layouts: &LayoutTable,
) -> Option<Vec<TypeId>> {
    let strong = |fs: &[dream_hir::FieldLayout]| -> Vec<TypeId> {
        fs.iter()
            .filter(|f| !f.is_weak && !f.is_unowned)
            .map(|f| f.ty)
            .collect()
    };
    Some(match interner.kind(ty) {
        TyKind::Array(e) => vec![*e],
        TyKind::Tuple(ts) => ts.clone(),
        TyKind::Struct(..) => strong(&layouts.get(ty)?.fields),
        TyKind::Union(..) => layouts
            .union(ty)?
            .variants
            .iter()
            .flat_map(|v| strong(&v.fields))
            .collect(),
        TyKind::Prim(_) | TyKind::Void | TyKind::Error | TyKind::Enum(_) => Vec::new(),
        TyKind::Object | TyKind::Interface(..) | TyKind::Func(..) | TyKind::Js => return None,
    })
}
