//! Flow-insensitive invalidation of borrowed snapshots before owning tokens are inserted.

use crate::analysis::effects::{Effect, Known, ModRef, ModRefTable, stmt_effects};
use crate::{Local, MirFunction, Place, Rvalue, Statement};
use dream_hir::LayoutTable;
use dream_types::{TypeId, TypeInterner};
use indexmap::IndexSet;
use std::collections::BTreeMap;

pub(super) fn invalidations(
    f: &MirFunction,
    interner: &TypeInterner,
    layouts: &LayoutTable,
    table: &ModRefTable,
    candidates: &IndexSet<u32>,
    forwarding: &IndexSet<u32>,
) -> ModRef {
    if f.blocks
        .iter()
        .any(|b| matches!(b.terminator, crate::Terminator::Await { .. }))
    {
        return ModRef::Top;
    }
    let mut effects = ModRef::default();
    for stmt in f.blocks.iter().flat_map(|block| &block.stmts) {
        stmt_effects(stmt, f, interner, |effect| {
            let summary = match effect {
                Effect::SlotStore(element) => ModRef::Known(Known {
                    slots: [element].into_iter().collect(),
                    ..Known::default()
                }),
                Effect::Call(callee) => table.call(callee),
                Effect::Iface(id, slot) => table.iface(id, slot),
                Effect::Ctor(def) => table.ctor(def),
                Effect::Top => ModRef::Top,
                _ => return,
            };
            if summary.may_destroy() {
                effects.join(table.del());
            }
            effects.join(&summary);
        });
        if table.statement_may_destroy(f, stmt, interner, layouts) {
            effects.join(table.del());
        }
    }
    let snapshots: IndexSet<u32> = f
        .blocks
        .iter()
        .flat_map(|block| &block.stmts)
        .filter_map(|stmt| match stmt {
            Statement::Assign(
                Place::Local(dest),
                Rvalue::Use(crate::Operand::Copy(Place::Field { .. } | Place::Index { .. }))
                | Rvalue::UnionField { .. },
            ) => Some(dest.0),
            _ => None,
        })
        .collect();
    let mut cleanup_types: BTreeMap<TypeId, bool> = BTreeMap::new();
    for (index, local) in f.locals.iter().enumerate() {
        let id = index as u32;
        let borrowed_parameter = f.params.contains(&Local(id)) && !local.is_take;
        // A candidate's extra token cannot be the last while its proven source owner is alive.
        // Other owning obligations can run cleanup before a later snapshot read.
        if local.is_ref
            || borrowed_parameter
            || candidates.contains(&id)
            || forwarding.contains(&id)
            || snapshots.contains(&id)
        {
            continue;
        }
        if *cleanup_types
            .entry(local.ty)
            .or_insert_with(|| table.may_run_del(local.ty, interner, layouts))
        {
            effects.join(table.del());
            break;
        }
    }
    effects
}
