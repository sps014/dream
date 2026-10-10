//! RC insertion separates analysis, block rewrites, exits and async resume cleanup.
mod awaits;
mod blocks;
mod exits;
mod helpers;
mod prepare;
use super::{
    modref::ModRefTable,
    value::{
        insert_early_value_drops, insert_value_struct_moves, mark_returned_value_locals_moved,
    },
};
use crate::MirFunction;
use crate::passes::MirPass;
use dream_types::{DefId, TypeInterner};
use indexmap::IndexSet;
pub struct RcInsertion;
impl RcInsertion {
    pub(crate) fn run_with_layouts(
        func: &mut MirFunction,
        interner: &TypeInterner,
        layouts: &dream_hir::LayoutTable,
        holds: &IndexSet<DefId>,
        modref: &ModRefTable,
    ) -> bool {
        RcInsertion.run_inner(
            func,
            interner,
            layouts,
            holds,
            modref,
            &mut crate::passes::FunctionAnalyses::default(),
        )
    }
    fn run_inner(
        &self,
        func: &mut MirFunction,
        interner: &TypeInterner,
        layouts: &dream_hir::LayoutTable,
        holds: &IndexSet<DefId>,
        modref: &ModRefTable,
        analyses: &mut crate::passes::FunctionAnalyses,
    ) -> bool {
        let state = prepare::State::new(func, interner, layouts, holds, modref, analyses);
        let mut changed = blocks::insert(func, interner, &state);
        insert_value_struct_moves(func, interner, &mut changed);
        insert_early_value_drops(func, interner, &mut changed, state.analysis.has_await);
        awaits::insert_await_resume_releases(func, interner, &mut changed);
        mark_returned_value_locals_moved(func, interner, &mut changed);
        exits::insert(func, interner, &state, &mut changed);
        changed
    }
}
impl MirPass for RcInsertion {
    fn preserves(&self) -> crate::passes::PreservedAnalyses {
        crate::passes::PreservedAnalyses::None
    }

    fn name(&self) -> &'static str {
        "rc-insertion"
    }

    fn transform(
        &self,
        func: &mut MirFunction,
        interner: &TypeInterner,
        layouts: &dream_hir::LayoutTable,
        analyses: &mut crate::passes::FunctionAnalyses,
    ) -> bool {
        self.run_inner(
            func,
            interner,
            layouts,
            &IndexSet::new(),
            &ModRefTable::default(),
            analyses,
        )
    }
}

#[cfg(test)]
use crate::{Const, Operand, Place, Rvalue, Statement, Terminator};
#[cfg(test)]
#[path = "../insertion_tests.rs"]
mod tests;
