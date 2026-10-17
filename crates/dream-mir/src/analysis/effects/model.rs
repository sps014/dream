//! Conservative heap-slot and ownership effects.

use dream_types::TypeId;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Known {
    /// `(base type, field)` slots that may be overwritten.
    pub(crate) fields: BTreeSet<(TypeId, u32)>,
    /// RC value types of array-element / global slots that may be overwritten.
    pub(crate) slots: BTreeSet<TypeId>,
    /// Some array-element slot of an unknown (possibly RC) type may be overwritten.
    pub(crate) any_slot: bool,
    /// May read an object's refcount or the live-object count (`Debug.ref_count` & co.).
    pub(crate) observes_rc: bool,
    /// An owning obligation may discharge a destructor-bearing graph, including implicit cleanup.
    pub(crate) may_destroy: bool,
}

/// What a region of code may do to the heap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ModRef {
    Known(Known),
    Top,
}

impl Default for ModRef {
    fn default() -> Self {
        ModRef::Known(Known::default())
    }
}

impl ModRef {
    pub(super) fn known_mut(&mut self) -> Option<&mut Known> {
        match self {
            ModRef::Known(k) => Some(k),
            ModRef::Top => None,
        }
    }

    pub(crate) fn join(&mut self, other: &ModRef) -> bool {
        match (&mut *self, other) {
            (ModRef::Top, _) => false,
            (_, ModRef::Top) => {
                *self = ModRef::Top;
                true
            }
            (ModRef::Known(a), ModRef::Known(b)) => {
                let before = (
                    a.fields.len(),
                    a.slots.len(),
                    a.any_slot,
                    a.observes_rc,
                    a.may_destroy,
                );
                a.fields.extend(b.fields.iter().copied());
                a.slots.extend(b.slots.iter().copied());
                a.any_slot |= b.any_slot;
                a.observes_rc |= b.observes_rc;
                a.may_destroy |= b.may_destroy;
                before
                    != (
                        a.fields.len(),
                        a.slots.len(),
                        a.any_slot,
                        a.observes_rc,
                        a.may_destroy,
                    )
            }
        }
    }

    pub(crate) fn writes_field(&self, ty: TypeId, field: u32) -> bool {
        match self {
            Self::Top => true,
            Self::Known(known) => known.fields.contains(&(ty, field)),
        }
    }

    /// Dynamic indices share one slot key per element type, including aliases of an array.
    pub(crate) fn writes_element(&self, element: TypeId) -> bool {
        match self {
            Self::Top => true,
            Self::Known(known) => known.any_slot || known.slots.contains(&element),
        }
    }

    pub(crate) fn may_destroy(&self) -> bool {
        match self {
            Self::Top => true,
            Self::Known(known) => known.may_destroy,
        }
    }

    /// True if a stored field slot is in `fields`, or is any field of a type in `any_field_of`.
    pub(crate) fn hits(
        &self,
        fields: &BTreeSet<(TypeId, u32)>,
        any_field_of: &BTreeSet<TypeId>,
    ) -> bool {
        match self {
            ModRef::Top => true,
            ModRef::Known(k) => k
                .fields
                .iter()
                .any(|key| fields.contains(key) || any_field_of.contains(&key.0)),
        }
    }

    /// No stores, ownership observations or possible destructor-bearing cleanup.
    pub(crate) fn is_quiet(&self) -> bool {
        match self {
            ModRef::Top => false,
            ModRef::Known(k) => {
                k.fields.is_empty()
                    && k.slots.is_empty()
                    && !k.any_slot
                    && !k.observes_rc
                    && !k.may_destroy
            }
        }
    }
}
