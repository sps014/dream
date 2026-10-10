use super::branches::elide_around_transparent_loops;
use super::branches::elide_transparent_diamonds;
use super::chains::elide_goto_chains;
use super::postdom::elide_postdom_transparent;
use crate::MirFunction;
use crate::passes::MirPass;
use dream_types::TypeInterner;

pub struct RcElision;

impl MirPass for RcElision {
    fn preserves(&self) -> crate::passes::PreservedAnalyses {
        crate::passes::PreservedAnalyses::ControlFlow
    }

    fn name(&self) -> &'static str {
        "rc-elision"
    }

    /// Cancels a `Retain(x)`/`Release(x)` pair on the same identity `x` when separated only by
    /// provably side-effect-free statements along:
    /// - unique-pred `Goto` chains (straight-line CFG),
    /// - transparent diamonds (both arms barrier-free, unique join),
    /// - transparent natural loops (body has no RC barriers; pair sandwiches the loop).
    ///
    /// The refcount an object carries at any point is observable (`Debug.ref_count`, `del()`). So
    /// **any** statement that could call into other code, allocate, or itself retain/release a
    /// (possibly aliased) object is a hard barrier. Only a small whitelist may pass through.
    /// Rule: never under-retain.
    fn transform(
        &self,
        func: &mut MirFunction,
        _interner: &TypeInterner,
        _layouts: &dream_hir::LayoutTable,
        analyses: &mut crate::passes::FunctionAnalyses,
    ) -> bool {
        let mut changed = false;
        // Fixpoint: diamond/loop/postdom elision can expose new Goto-chain pairs and vice versa.
        const MAX_ROUNDS: usize = 8;
        for iteration in 0..MAX_ROUNDS {
            let mut round = false;
            round |= elide_goto_chains(func, analyses);
            round |= elide_transparent_diamonds(func, analyses);
            round |= elide_around_transparent_loops(func, analyses);
            round |= elide_postdom_transparent(func, analyses);
            if !round {
                break;
            }
            changed = true;
            if iteration + 1 == MAX_ROUNDS {
                crate::passes::limits::reached(
                    crate::passes::limits::Limit::RcElision,
                    MAX_ROUNDS,
                    Some(func),
                );
            }
        }
        changed
    }
}
