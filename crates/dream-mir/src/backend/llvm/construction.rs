use crate::{Mir, Rvalue, Statement};
use dream_types::DefId;
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
pub(super) enum InitMode {
    Ordinary,
    Fresh,
}
pub(super) fn fresh_name(name: &str) -> String {
    format!("{name}__fresh_init")
}

pub(super) fn fresh_initializers(mir: &Mir) -> BTreeSet<DefId> {
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
            ) if ctor.field_init => Some(ctor.def),
            _ => None,
        })
        .collect()
}
