//! Finite strong-owner paths rooted in an existing fresh local token.

use crate::analysis::effects::ModRefTable;
use crate::analysis::ownership::{Location, constant_index};
use crate::{Const, Local, MirFunction, Operand, Place, Rvalue, Statement};
use dream_hir::LayoutTable;
use dream_types::TypeInterner;
use std::collections::BTreeSet;

pub(super) struct Coverage {
    pub(super) root: Local,
    pub(super) locals: BTreeSet<Local>,
    pub(super) slots: Vec<Location>,
}

impl Coverage {
    pub(super) fn find(
        f: &MirFunction,
        source: Local,
        i: &TypeInterner,
        layouts: &LayoutTable,
        effects: &ModRefTable,
    ) -> Option<Self> {
        let mut coverage = Self {
            root: source,
            locals: BTreeSet::new(),
            slots: vec![],
        };
        loop {
            let local = coverage.root;
            if !coverage.locals.insert(local)
                || f.params.contains(&local)
                || f.locals[local.0 as usize].is_ref
                || i.is_shared_type(f.local_ty(local))
            {
                return None;
            }
            let mut definitions = f
                .blocks
                .iter()
                .flat_map(|b| &b.stmts)
                .filter_map(|s| match s {
                    Statement::Assign(Place::Local(d), rv)
                        if *d == local
                            && !matches!(rv, Rvalue::Use(Operand::Const(Const::Null))) =>
                    {
                        Some(rv)
                    }
                    _ => None,
                });
            let rv = definitions.next()?;
            if definitions.next().is_some() {
                return None;
            }
            if fresh(rv, layouts) && !f.locals[local.0 as usize].is_cursor {
                return Some(coverage);
            }
            // An owning intermediate can have a consuming alias that pays for its token.
            // Follow only cursor links whose lifetime already depends on another owner.
            if !f.locals[local.0 as usize].is_cursor
                || effects.may_run_del(f.local_ty(local), i, layouts)
            {
                return None;
            }
            match rv {
                Rvalue::Use(Operand::Copy(Place::Field { base, field })) => {
                    let link = layouts.get(f.local_ty(*base))?.fields.get(*field)?;
                    if !i.is_reference(f.local_ty(*base))
                        || !i.is_reference(link.ty)
                        || link.is_weak
                        || link.is_unowned
                    {
                        return None;
                    }
                    coverage.slots.push(Location::Field {
                        base: *base,
                        field: *field as u32,
                    });
                    coverage.root = *base;
                }
                Rvalue::Use(Operand::Copy(Place::Index { base, index, .. })) => {
                    let dream_types::TyKind::Array(element) = i.kind(f.local_ty(*base)) else {
                        return None;
                    };
                    if !i.is_reference(*element) {
                        return None;
                    }
                    coverage.slots.push(Location::Element {
                        base: *base,
                        index: constant_index(index),
                    });
                    coverage.root = *base;
                }
                _ => return None,
            }
        }
    }
}

fn fresh(rv: &Rvalue, layouts: &LayoutTable) -> bool {
    match rv {
        Rvalue::Concat(args) => {
            args.len() >= 2
                && args
                    .iter()
                    .any(|op| matches!(op, Operand::Const(Const::Str(s)) if !s.is_empty()))
        }
        Rvalue::ConcatInt { .. }
        | Rvalue::ArrayLit { .. }
        | Rvalue::ArrayNew {
            closure_env: false, ..
        } => true,
        Rvalue::New { ty, ctor: None, .. } => layouts
            .get(*ty)
            .is_some_and(|l| !l.fields.iter().any(|f| f.is_weak || f.is_unowned)),
        _ => false,
    }
}
