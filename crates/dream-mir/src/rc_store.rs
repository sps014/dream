//! Which container stores adopt their source's count instead of retaining it. The C backend's
//! `rc_store` and passes that spell such a store out as explicit RC statements share this rule.

use crate::Rvalue;
use dream_types::{TyKind, TypeInterner};

/// A `Move`, a fresh allocation or a boxing cast: the slot takes the value's `+1`.
pub(crate) fn store_adopts(interner: &TypeInterner, rv: &Rvalue) -> bool {
    matches!(rv, Rvalue::Move { .. }) || rvalue_allocates(rv) || boxes_into_object(interner, rv)
}

/// `int` → `object` and friends allocate the box they yield (`dream_box_*`), so the slot they are
/// stored into takes that fresh +1. Retaining instead would leave the box's original reference with
/// no owner to drop it. A `string` → `object` cast is a no-op pun, not a box, and is excluded by the
/// reference-counted check.
pub(crate) fn boxes_into_object(interner: &TypeInterner, rv: &Rvalue) -> bool {
    let Rvalue::Cast(_, from, to) = rv else {
        return false;
    };
    matches!(interner.kind(*to), TyKind::Object)
        && matches!(interner.kind(*from), TyKind::Prim(_))
        && !interner.is_rc_tracked(*from)
}

pub(crate) fn rvalue_allocates(rv: &Rvalue) -> bool {
    // Fewer than two parts never reaches `dream_concat_n`: one part lowers to the operand itself
    // and zero to an interned literal, neither of which is a reference this store may adopt.
    if let Rvalue::Concat(parts) = rv {
        return parts.len() >= 2;
    }
    matches!(
        rv,
        Rvalue::New { .. }
            | Rvalue::Tuple { .. }
            | Rvalue::UnionNew { .. }
            | Rvalue::Call { .. }
            | Rvalue::InterfaceCall { .. }
            | Rvalue::IndirectCall { .. }
            | Rvalue::ArrayLit { .. }
            | Rvalue::ArrayNew { .. }
            | Rvalue::ArrayRealloc { .. }
            // `dream_concat_str_int_str` mallocs its result. The in-place `_into` variants reuse
            // their destination, but `emit_into` only targets locals, never a slot stored here.
            | Rvalue::ConcatInt { .. }
            // `dream_from_bytes` hands back a heap box. A local or field destination copies out of
            // it and frees it in `store_from_bytes_value`, but an array element reaches the generic
            // store instead, and the box's type is not reference-counted so no scope exit covers it.
            | Rvalue::FromBytes { .. }
    )
}
