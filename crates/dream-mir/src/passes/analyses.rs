//! Function-scoped CFG caches. Statement facts deliberately stay outside this cache: changing a
//! definition can invalidate range, liveness or ownership facts without changing any CFG edge.

use super::{cfg, MirPass};
use crate::{BlockId, MirFunction};
use dream_hir::LayoutTable;
use dream_types::TypeInterner;
use std::rc::Rc;

pub use super::cfg::{DomTree, NaturalLoop, PostDomTree};

/// A pass must declare this even when it does not consume analyses itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreservedAnalyses {
    None,
    /// Block identities, entry and ordered successor edges are unchanged. Terminator operands
    /// and statements may change; no value-dependent facts are cached here.
    ControlFlow,
}

/// Owned by one function pipeline, never shared across functions or module transformations.
/// Handles are immutable snapshots; after a CFG edit callers must discard old handles and call
/// `invalidate` before querying again, including inside a pass that performs several edits.
#[derive(Default)]
pub struct FunctionAnalyses {
    preds: Option<Rc<Vec<Vec<BlockId>>>>,
    rpo: Option<Rc<Vec<BlockId>>>,
    dom: Option<Rc<DomTree>>,
    postdom: Option<Rc<PostDomTree>>,
    loops: Option<Rc<Vec<NaturalLoop>>>,
}

impl FunctionAnalyses {
    pub fn predecessors(&mut self, func: &MirFunction) -> Rc<Vec<Vec<BlockId>>> {
        self.preds
            .get_or_insert_with(|| Rc::new(cfg::predecessors(func)))
            .clone()
    }

    pub fn reverse_postorder(&mut self, func: &MirFunction) -> Rc<Vec<BlockId>> {
        self.rpo
            .get_or_insert_with(|| Rc::new(cfg::reverse_postorder(func)))
            .clone()
    }

    pub fn dominators(&mut self, func: &MirFunction) -> Rc<DomTree> {
        if self.dom.is_none() {
            let preds = self.predecessors(func);
            let rpo = self.reverse_postorder(func);
            self.dom = Some(Rc::new(DomTree::compute(func, &preds, &rpo)));
        }
        self.dom.as_ref().unwrap().clone()
    }

    pub fn postdominators(&mut self, func: &MirFunction) -> Rc<PostDomTree> {
        self.postdom
            .get_or_insert_with(|| Rc::new(PostDomTree::new(func)))
            .clone()
    }

    pub fn natural_loops(&mut self, func: &MirFunction) -> Rc<Vec<NaturalLoop>> {
        if self.loops.is_none() {
            let preds = self.predecessors(func);
            let dom = self.dominators(func);
            self.loops = Some(Rc::new(cfg::natural_loops(func, &preds, &dom)));
        }
        self.loops.as_ref().unwrap().clone()
    }

    pub fn invalidate(&mut self) {
        *self = Self::default();
    }

    pub(super) fn run_pass<P: MirPass + ?Sized>(
        &mut self,
        pass: &P,
        func: &mut MirFunction,
        interner: &TypeInterner,
        layouts: &LayoutTable,
    ) -> bool {
        let before = (cfg!(test) || crate::verify::enabled()).then(|| CfgShape::of(func));
        let changed = pass.transform(func, interner, layouts, self);
        if let Some(before) = before {
            let cfg_changed = before != CfgShape::of(func);
            assert!(
                !cfg_changed || changed,
                "MIR pass {} changed control flow but reported no change",
                pass.name()
            );
            assert!(
                !cfg_changed || pass.preserves() == PreservedAnalyses::None,
                "MIR pass {} violated its control-flow preservation contract",
                pass.name()
            );
        }
        if changed && pass.preserves() == PreservedAnalyses::None {
            self.invalidate();
        }
        changed
    }
}

#[derive(PartialEq, Eq)]
struct CfgShape {
    entry: BlockId,
    successors: Vec<Vec<BlockId>>,
}

impl CfgShape {
    fn of(func: &MirFunction) -> Self {
        Self {
            entry: func.entry,
            successors: func
                .blocks
                .iter()
                .map(|b| b.terminator.successors())
                .collect(),
        }
    }
}

#[cfg(test)]
#[path = "analyses_tests.rs"]
mod tests;
