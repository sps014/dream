//! Whether destroying a value can run a user `del`. Such parameters keep the sink mode: a sink
//! dies when the callee returns, while a borrowed argument with a `del` waits for the caller's
//! block end, so inferring borrow would move an observable destructor.

use dream_hir::{FieldLayout, LayoutTable};
use dream_types::{TyKind, TypeId, TypeInterner};
use indexmap::{IndexMap, IndexSet};

pub(super) struct Destructors<'p> {
    interner: &'p TypeInterner,
    layouts: &'p LayoutTable,
    memo: IndexMap<TypeId, bool>,
}

impl<'p> Destructors<'p> {
    pub(super) fn new(interner: &'p TypeInterner, layouts: &'p LayoutTable) -> Self {
        Self {
            interner,
            layouts,
            memo: IndexMap::new(),
        }
    }

    pub(super) fn may_run(&mut self, ty: TypeId) -> bool {
        if let Some(&known) = self.memo.get(&ty) {
            return known;
        }
        let runs = self.walk(ty);
        self.memo.insert(ty, runs);
        runs
    }

    fn walk(&self, root: TypeId) -> bool {
        let strong = |fields: &[FieldLayout]| -> Vec<TypeId> {
            fields
                .iter()
                .filter(|f| !f.is_weak && !f.is_unowned)
                .map(|f| f.ty)
                .collect()
        };
        let mut seen = IndexSet::new();
        let mut stack = vec![root];
        while let Some(ty) = stack.pop() {
            if !seen.insert(ty) {
                continue;
            }
            match self.interner.kind(ty) {
                TyKind::Prim(_) | TyKind::Void | TyKind::Error | TyKind::Enum(_) => {}
                TyKind::Object | TyKind::Interface(..) | TyKind::Func(..) | TyKind::Js => {
                    return true;
                }
                TyKind::Array(element) => stack.push(*element),
                TyKind::Tuple(elements) => stack.extend(elements.iter().copied()),
                TyKind::Struct(..) => match self.layouts.get(ty) {
                    Some(layout) if !layout.has_destructor() => {
                        stack.extend(strong(&layout.fields))
                    }
                    _ => return true,
                },
                TyKind::Union(..) => match self.layouts.union(ty) {
                    Some(layout) if !layout.has_destructor() => {
                        stack.extend(layout.variants.iter().flat_map(|v| strong(&v.fields)))
                    }
                    _ => return true,
                },
            }
        }
        false
    }
}
