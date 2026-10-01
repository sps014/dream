//! Store barriers for managed references, including inline value payloads.

use super::fx::{Fx, V};
use crate::backend::shared::ValueLocalKind;
use crate::{Local, Place};
use dream_types::TypeId;

impl Fx<'_, '_> {
    pub(super) fn publish_store(&mut self, place: &Place, ty: TypeId, rhs: &V) {
        let owner = match place {
            Place::Field { base, .. } if !self.is_value(self.f.local_ty(*base)) => {
                self.read_local(*base)
            }
            Place::Index { base, .. }
                if matches!(
                    self.interner.kind(self.f.local_ty(*base)),
                    dream_types::TyKind::Array(_)
                ) =>
            {
                self.read_local(*base)
            }
            Place::Field { base, .. } | Place::Local(base) if self.private_value(*base) => return,
            Place::Local(_) => return,
            // Globals and raw/ref interiors have no recoverable owning heap header.
            _ => V::i32(0),
        };
        self.publish_refs(ty, rhs, &owner);
    }

    fn private_value(&self, local: Local) -> bool {
        matches!(
            self.value_frame.kind(local),
            Some(ValueLocalKind::Owning | ValueLocalKind::Param)
        )
    }
}
