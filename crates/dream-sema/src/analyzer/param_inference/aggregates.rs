//! Value aggregates (structs, unions, tuples) are not reference counted as envelopes, but a sink
//! aggregate parameter still releases every reference field it holds. Such parameters are
//! inference candidates just like references.

use dream_hir::{FieldLayout, LayoutTable};
use dream_types::{TyKind, TypeId, TypeInterner};
use indexmap::IndexSet;

/// True when `ty` is a value aggregate that owns at least one reference-counted field. An
/// aggregate with a weak or unowned field anywhere inside stays out: its sink release also
/// maintains back-link bookkeeping that inference does not model.
pub(super) fn owns_references(interner: &TypeInterner, layouts: &LayoutTable, ty: TypeId) -> bool {
    if interner.is_rc_tracked(ty) {
        return false;
    }
    let mut seen = IndexSet::new();
    let mut stack = vec![ty];
    let mut owns = false;
    while let Some(ty) = stack.pop() {
        if !seen.insert(ty) {
            continue;
        }
        if interner.is_rc_tracked(ty) {
            owns = true;
            continue;
        }
        let fields: Vec<&FieldLayout> = match interner.kind(ty) {
            TyKind::Struct(..) => match layouts.get(ty) {
                Some(layout) => layout.fields.iter().collect(),
                None => return false,
            },
            TyKind::Union(..) => match layouts.union(ty) {
                Some(layout) => layout.variants.iter().flat_map(|v| &v.fields).collect(),
                None => return false,
            },
            TyKind::Tuple(elements) => {
                stack.extend(elements.iter().copied());
                continue;
            }
            _ => continue,
        };
        if fields.iter().any(|f| f.is_weak || f.is_unowned) {
            return false;
        }
        stack.extend(fields.iter().map(|f| f.ty));
    }
    owns
}
