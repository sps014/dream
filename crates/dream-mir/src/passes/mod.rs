//! The MIR optimization pass manager and passes.

mod abc;
mod algebraic;
mod analyses;
mod autovec;
#[cfg(test)]
mod await_facts_tests;
mod cfg;
mod const_fold;
mod construction;
mod dce;
mod devirt;
mod dse;
mod dump;
mod frame_alloc;
#[cfg(test)]
mod frame_alloc_tests;
pub(crate) mod funcbox_abi;
mod global_prop;
mod gvn;
pub(crate) mod inline;
mod iv;
mod licm;
mod limits;
#[cfg(test)]
mod limits_tests;
mod loop_unroll;
mod manager;
mod module_pipeline;
mod overflow_elim;
mod ownership_args;
#[cfg(test)]
mod ownership_args_tests;
mod prop;
pub(crate) mod rc;
mod sccp;
mod simplify_cfg;
mod slice_measure;
mod sroa;
mod str_cursor;
mod tco;
mod unique_region;
mod value_borrow;

pub use abc::Abc;
pub use algebraic::Algebraic;
pub use analyses::{DomTree, FunctionAnalyses, NaturalLoop, PostDomTree, PreservedAnalyses};
pub use autovec::Autovec;
pub use const_fold::ConstFold;
pub use dce::Dce;
pub(crate) use dce::is_pure;
pub use devirt::Devirt;
pub use dse::Dse;
pub use dump::{
    MirDump, MirDumpFile, MirDumpSpec, STAGE_FIXPOINT, STAGE_LOWER, dumpable_pass_names,
};
pub use funcbox_abi::FuncboxAbi;
pub use global_prop::GlobalProp;
pub use gvn::Gvn;
pub use inline::Inliner;
pub use iv::IvCanon;
pub use licm::Licm;
pub use loop_unroll::LoopUnroll;
pub use manager::PassManager;
pub use module_pipeline::{
    optimize_module, optimize_module_opts, prepare_debug_module, run_function_pipelines,
    run_late_module_passes,
};
pub use overflow_elim::OverflowElim;
pub use prop::CopyConstProp;
pub use rc::{HopElision, RcElision, RcInsertion, ReleaseSink};
pub(crate) use rc::{rvalue_reads_local, stmt_reads_local};
pub use sccp::Sccp;
pub use simplify_cfg::SimplifyCfg;
pub use sroa::{ExpandSimpleCtors, Sroa, SroaManaged};
pub use str_cursor::StrCursor;
pub use tco::Tco;
pub use unique_region::UniqueRegion;

use super::{Mir, MirFunction};
use dream_types::TypeInterner;

/// A single function-level MIR transformation.
pub trait MirPass {
    fn name(&self) -> &'static str;
    fn preserves(&self) -> PreservedAnalyses;

    /// The transformation implementation. CFG-editing passes must invalidate before making a
    /// second analysis query on the edited graph. Call `run` for a standalone managed invocation.
    fn transform(
        &self,
        func: &mut MirFunction,
        interner: &TypeInterner,
        layouts: &dream_hir::LayoutTable,
        analyses: &mut FunctionAnalyses,
    ) -> bool;

    /// Runs with a fresh function-scoped cache. The return value drives the manager's fixpoint.
    fn run(&self, func: &mut MirFunction, interner: &TypeInterner) -> bool {
        self.run_with_layouts(func, interner, &dream_hir::LayoutTable::default())
    }

    fn run_with_layouts(
        &self,
        func: &mut MirFunction,
        interner: &TypeInterner,
        layouts: &dream_hir::LayoutTable,
    ) -> bool {
        FunctionAnalyses::default().run_pass(self, func, interner, layouts)
    }
}

/// A whole-program transformation (needs to see every function at once, e.g. inlining). Distinct
/// from [`MirPass`], which is function-local.
pub trait ModulePass {
    fn name(&self) -> &'static str;
    /// Runs the pass over the whole module. Returns `true` if it changed anything.
    fn run(&self, mir: &mut Mir, interner: &TypeInterner) -> bool;
}
