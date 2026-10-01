//! Reference-bearing inline values participate in region provenance, but not envelope ARC.

use dream_hir::LayoutTable;
use dream_types::{TyKind, TypeId, TypeInterner};
use std::collections::BTreeSet;

pub(super) struct RefTypes(BTreeSet<TypeId>);

impl RefTypes {
    pub fn new(layouts: &LayoutTable, interner: &TypeInterner) -> Self {
        let mut refs: BTreeSet<_> = interner
            .iter_kinds()
            .filter_map(|(ty, _)| interner.is_rc_tracked(ty).then_some(ty))
            .collect();
        loop {
            let size = refs.len();
            for (ty, kind) in interner.iter_kinds() {
                if !interner.is_value_type(ty) {
                    continue;
                }
                let fields = layouts.get(ty).into_iter().flat_map(|l| &l.fields).chain(
                    layouts
                        .union(ty)
                        .into_iter()
                        .flat_map(|u| &u.variants)
                        .flat_map(|v| &v.fields),
                );
                let contains = fields.into_iter().any(|f| refs.contains(&f.ty))
                    || matches!(kind, TyKind::Tuple(fields) if fields.iter().any(|ty| refs.contains(ty)));
                if contains {
                    refs.insert(ty);
                }
            }
            if refs.len() == size {
                return Self(refs);
            }
        }
    }

    pub fn contains(&self, ty: TypeId) -> bool {
        self.0.contains(&ty)
    }
}
