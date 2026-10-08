use crate::{AllocPolicy, Mir, MirFunction, Place, Rvalue, Statement, Terminator};
use dream_types::DefId;
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
pub(super) enum InitMode {
    Ordinary,
    Private,
    PrivateBuilder,
    TrackedBuilder,
    Tracked,
}

pub(super) fn tracked_name(name: &str) -> String {
    format!("{name}__tracked_init")
}

pub(super) fn tracked_initializers(mir: &Mir) -> BTreeSet<DefId> {
    mir.functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.stmts)
        .filter_map(|s| match s {
            Statement::Assign(
                _,
                Rvalue::New {
                    ctor: Some(ctor), ..
                },
            ) if ctor.batched => Some(ctor.def),
            _ => None,
        })
        .collect()
}

pub(super) fn private_name(name: &str) -> String {
    format!("{name}__private_init")
}

pub(super) fn private_initializers(mir: &Mir) -> BTreeSet<DefId> {
    mir.functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.stmts)
        .filter_map(|s| match s {
            Statement::Assign(
                _,
                Rvalue::New {
                    ctor: Some(ctor),
                    policy: AllocPolicy::Private,
                    ..
                },
            ) if ctor.batched => Some(ctor.def),
            _ => None,
        })
        .collect()
}

pub(super) fn builder_name(name: &str) -> String {
    format!("{name}__private_graph")
}

pub(super) fn tracked_builder_name(name: &str) -> String {
    format!("{name}__tracked_graph")
}

pub(super) fn only_fresh_edges(mir: &Mir, builder: &MirFunction) -> bool {
    let mut pending = vec![builder.def];
    let mut seen = BTreeSet::new();
    while let Some(def) = pending.pop() {
        if !seen.insert(def) {
            continue;
        }
        let Some(f) = mir.functions.iter().find(|f| f.def == def) else {
            return false;
        };
        for block in &f.blocks {
            for statement in &block.stmts {
                match statement {
                    // Ordinary field mutation needs distinct-component checks, even inside
                    // a fresh builder. Only verified constructor writes skip those checks.
                    Statement::Assign(Place::Field { .. }, _) => return false,
                    Statement::Assign(_, Rvalue::Call { callee, .. }) => pending.push(callee.def),
                    _ => {}
                }
            }
            if let Terminator::TailCall { callee, .. } = &block.terminator {
                pending.push(callee.def);
            }
        }
    }
    true
}
