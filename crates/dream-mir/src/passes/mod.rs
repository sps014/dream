//! The MIR optimization pass manager and passes.

mod abc;
mod algebraic;
mod autovec;
mod cfg;
mod clear_dead_gc;
mod const_fold;
mod dce;
mod devirt;
mod dse;
mod global_prop;
mod gvn;
mod inline;
mod iv;
mod licm;
mod loop_unroll;
mod prop;
mod sccp;
mod simplify_cfg;
mod sroa;
mod tco;

pub use abc::Abc;
pub use algebraic::Algebraic;
pub use autovec::Autovec;
pub use clear_dead_gc::ClearDeadGcRoots;
pub use const_fold::ConstFold;
pub(crate) use dce::is_pure;
pub use dce::Dce;
pub use devirt::Devirt;
pub use dse::Dse;
pub use global_prop::GlobalProp;
pub use gvn::Gvn;
pub use inline::Inliner;
pub use iv::IvCanon;
pub use licm::Licm;
pub use loop_unroll::LoopUnroll;
pub use prop::CopyConstProp;
pub use sccp::Sccp;
pub use simplify_cfg::SimplifyCfg;
pub use sroa::{ExpandSimpleCtors, Sroa};
pub use tco::Tco;

use super::{Mir, MirFunction};
use dream_types::TypeInterner;

/// A single function-level MIR transformation.
pub trait MirPass {
    fn name(&self) -> &'static str;
    /// Runs the pass over one function. Returns `true` if it changed anything (drives the
    /// fixpoint loop in [`PassManager::run`]).
    fn run(&self, func: &mut MirFunction, interner: &TypeInterner) -> bool;
}

/// A whole-program transformation (needs to see every function at once, e.g. inlining). Distinct
/// from [`MirPass`], which is function-local.
pub trait ModulePass {
    fn name(&self) -> &'static str;
    /// Runs the pass over the whole module. Returns `true` if it changed anything.
    fn run(&self, mir: &mut Mir, interner: &TypeInterner) -> bool;
}

/// Runs a configured pipeline of passes to a fixpoint over each function.
pub struct PassManager {
    passes: Vec<Box<dyn MirPass>>,
    max_iterations: usize,
    /// When true (release/opt builds), [`ClearDeadGcRoots`] runs once after the fixpoint so dead
    /// heap refs stop keeping objects alive. Debug-info builds leave those locals intact so DAP
    /// can still decode named variables after their last MIR use.
    clear_dead_gc: bool,
}

impl PassManager {
    pub fn new() -> Self {
        PassManager {
            passes: Vec::new(),
            max_iterations: 16,
            clear_dead_gc: true,
        }
    }

    /// The default optimization pipeline, ordered so cheap simplifications expose work for the
    /// later ones (prop -> fold -> algebraic -> gvn -> simplify-cfg -> dce).
    pub fn default_pipeline() -> Self {
        let mut pm = PassManager::new();
        pm.add(CopyConstProp);
        pm.add(GlobalProp);
        pm.add(Sccp);
        pm.add(ConstFold);
        pm.add(Algebraic);
        pm.add(Gvn);
        pm.add(Licm);
        pm.add(Abc);
        pm.add(IvCanon);
        pm.add(Autovec);
        pm.add(LoopUnroll);
        pm.add(Sroa);
        pm.add(Dse);
        pm.add(SimplifyCfg);
        pm.add(Tco);
        pm.add(Dce);
        pm
    }

    /// A minimal, value-preserving pipeline for debug-info builds. It deliberately omits every pass
    /// that can eliminate, fold, or coalesce user locals (const/copy propagation, SCCP, GVN, DCE,
    /// DSE), and skips [`ClearDeadGcRoots`], so each declared variable still lives in a distinct
    /// slot the debugger can read at every statement. Only the CFG is tidied.
    pub fn debug_pipeline() -> Self {
        let mut pm = PassManager::new();
        pm.clear_dead_gc = false;
        pm.add(SimplifyCfg);
        pm
    }

    pub fn add(&mut self, pass: impl MirPass + 'static) {
        self.passes.push(Box::new(pass));
    }

    /// Runs every pass repeatedly until none reports a change (or the iteration cap is hit), then
    /// (unless this is the debug pipeline) clears dead GC-tracked locals once so DCE cannot delete
    /// the nulling stores.
    pub fn run(&self, func: &mut MirFunction, interner: &TypeInterner) {
        for _ in 0..self.max_iterations {
            let mut changed = false;
            for pass in &self.passes {
                changed |= pass.run(func, interner);
            }
            if !changed {
                break;
            }
        }
        if self.clear_dead_gc {
            let _ = ClearDeadGcRoots.run(func, interner);
        }
    }
}

impl Default for PassManager {
    fn default() -> Self {
        PassManager::new()
    }
}

/// Whole-module optimization: aggressive tree-shaking interleaved with function inlining, run to a
/// fixpoint. After inlining, [`crate::driver`] runs the per-function [`PassManager`].
pub fn optimize_module(mir: &mut Mir, interner: &TypeInterner) {
    optimize_module_opts(mir, interner, true)
}

/// Like [`optimize_module`], but `inline` can be disabled. Debug-info builds turn inlining off so
/// each user function keeps its own body (and thus its own call-stack frame + local variables),
/// which the interactive debugger relies on. Dead-function pruning still runs in both modes.
pub fn optimize_module_opts(mir: &mut Mir, interner: &TypeInterner, inline: bool) {
    const MAX_ROUNDS: usize = 8;
    crate::prune_module(mir, interner);
    if !inline {
        let _ = ExpandSimpleCtors.run(mir, interner);
        return;
    }
    let _ = Devirt.run(mir, interner);
    let inliner = Inliner;
    for _ in 0..MAX_ROUNDS {
        let changed = inliner.run(mir, interner);
        // Drop callees left with no remaining call sites after inlining (plus their transitively
        // dead callees), then loop: the smaller module may expose more inlining.
        crate::prune_module(mir, interner);
        if !changed {
            break;
        }
        let _ = Devirt.run(mir, interner);
    }
    // After inlining, lower simple user-ctors to `New { ctor: None }` + field stores so per-function
    // SROA can promote non-escaping Acc(n)-style instances.
    let _ = ExpandSimpleCtors.run(mir, interner);
}
