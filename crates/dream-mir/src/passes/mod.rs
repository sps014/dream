//! The MIR optimization pass manager and passes.

mod abc;
mod algebraic;
mod autovec;
mod cfg;
mod const_fold;
mod dce;
mod devirt;
mod dse;
mod dump;
mod frame_alloc;
mod funcbox_abi;
mod param_modes;
#[cfg(test)]
mod param_modes_tests;
mod global_prop;
mod gvn;
pub(crate) mod inline;
mod iv;
mod licm;
mod loop_unroll;
mod overflow_elim;
mod prop;
pub(crate) mod rc;
mod sccp;
mod slice_measure;
mod str_cursor;
mod simplify_cfg;
mod sroa;
mod tco;
mod unique_region;

pub use abc::Abc;
pub use algebraic::Algebraic;
pub use autovec::Autovec;
pub use const_fold::ConstFold;
pub(crate) use dce::is_pure;
pub use dce::Dce;
pub use devirt::Devirt;
pub use dse::Dse;
pub use dump::{
    dumpable_pass_names, MirDump, MirDumpFile, MirDumpSpec, STAGE_FIXPOINT, STAGE_LATE, STAGE_LOWER,
};
pub use funcbox_abi::FuncboxAbi;
pub use param_modes::ParamModes;
pub use global_prop::GlobalProp;
pub use gvn::Gvn;
pub use inline::Inliner;
pub use iv::IvCanon;
pub use licm::Licm;
pub use loop_unroll::LoopUnroll;
pub use overflow_elim::OverflowElim;
pub use prop::CopyConstProp;
pub(crate) use rc::{container_move_locals, rvalue_reads_local, stmt_reads_local};
pub use rc::{HopElision, RcElision, RcInsertion, RcLastUseRepair};
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
}

impl PassManager {
    pub fn new() -> Self {
        PassManager {
            passes: Vec::new(),
            max_iterations: 16,
        }
    }

    /// The default optimization pipeline, ordered so cheap simplifications expose work for the
    /// later ones (prop -> fold -> algebraic -> overflow-elim -> gvn -> simplify-cfg -> dce, then RC elision).
    pub fn default_pipeline() -> Self {
        let mut pm = PassManager::new();
        pm.add(CopyConstProp);
        pm.add(GlobalProp);
        pm.add(Sccp);
        pm.add(ConstFold);
        pm.add(Algebraic);
        pm.add(OverflowElim);
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
        pm.add(HopElision);
        pm.add(RcElision);
        pm.add(StrCursor);
        // RC *insertion* is a module-wide phase that must run once before inlining (see
        // `optimize_module`); the per-function pipeline only *elides* redundant RC. Running
        // RcInsertion here would double-insert retains/releases, so guard against that regression.
        debug_assert!(
            pm.passes.iter().all(|p| p.name() != "rc-insertion"),
            "per-function pipeline must not contain RcInsertion (RC is inserted module-wide first)"
        );
        debug_assert!(
            pm.passes.iter().any(|p| p.name() == "rc-elision"),
            "per-function pipeline is expected to clean up RC with RcElision"
        );
        pm
    }

    /// Native C emit: same as [`Self::default_pipeline`] without wasm `v128` autovec so clang
    /// can vectorize scalar loops at `DREAM_F32_LANES` (AVX2 = 8).
    pub fn native_c_pipeline() -> Self {
        let mut pm = PassManager::new();
        pm.add(CopyConstProp);
        pm.add(GlobalProp);
        pm.add(Sccp);
        pm.add(ConstFold);
        pm.add(Algebraic);
        pm.add(OverflowElim);
        pm.add(Gvn);
        pm.add(Licm);
        pm.add(Abc);
        pm.add(LoopUnroll);
        pm.add(Sroa);
        pm.add(Dse);
        pm.add(SimplifyCfg);
        pm.add(Tco);
        pm.add(Dce);
        pm.add(HopElision);
        pm.add(RcElision);
        pm.add(StrCursor);
        debug_assert!(pm.passes.iter().all(|p| p.name() != "autovec"));
        pm
    }

    /// A pipeline safe for `mir.polls` (async coroutine bodies).
    /// Omits `RcElision` (which is unsafe across `Await` suspend points), `Tco` (tail calls are invalid
    /// in coroutines), and `SimplifyCfg` (which can mangle the `$__pc` dispatch block).
    pub fn async_poll_pipeline() -> Self {
        let mut pm = PassManager::new();
        pm.add(GlobalProp);
        pm.add(Sccp);
        pm.add(ConstFold);
        pm.add(Algebraic);
        pm.add(OverflowElim);
        pm.add(Gvn);
        pm.add(Licm);
        pm.add(Abc);
        pm.add(LoopUnroll);
        pm.add(Sroa);
        pm.add(Dse);
        pm.add(Dce);
        pm.add(HopElision);
        pm.add(StrCursor);
        pm
    }

    /// A minimal, value-preserving pipeline for debug-info builds. It deliberately omits every pass
    /// that can eliminate, fold, or coalesce user locals (const/copy propagation, SCCP, GVN, DCE,
    /// DSE), so each declared variable still lives in a distinct slot the debugger can read at every
    /// statement. Only redundant RC is elided (a value-neutral cleanup) and the CFG is tidied.
    pub fn debug_pipeline() -> Self {
        let mut pm = PassManager::new();
        pm.add(SimplifyCfg);
        pm.add(RcElision);
        pm
    }

    pub fn add(&mut self, pass: impl MirPass + 'static) {
        self.passes.push(Box::new(pass));
    }

    /// Runs every pass repeatedly until none reports a change (or the iteration cap is hit).
    pub fn run(&self, func: &mut MirFunction, interner: &TypeInterner) {
        self.run_dumped(func, interner, &mut MirDump::disabled());
    }

    /// [`Self::run`], reporting every pass run to `dump` (`--emit-mir=after:<pass>[,each]`).
    pub fn run_dumped(&self, func: &mut MirFunction, interner: &TypeInterner, dump: &mut MirDump) {
        for iteration in 0..self.max_iterations {
            let mut changed = false;
            for pass in &self.passes {
                let pass_changed = pass.run(func, interner);
                if dump.is_active() {
                    dump.function_pass(pass.as_ref(), iteration, pass_changed, func, interner);
                }
                changed |= pass_changed;
            }
            if !changed {
                break;
            }
        }
    }
}

impl Default for PassManager {
    fn default() -> Self {
        PassManager::new()
    }
}

/// Whole-module optimization: reference-counting insertion, then aggressive tree-shaking interleaved
/// with function inlining, run to a fixpoint.
///
/// Crucially, `RcInsertion` runs *before* inlining. Dream has deterministic, reference-counted
/// destruction, so a local reference's lifetime must end at the point its owning function returns —
/// not at the caller's scope exit. Inserting RC first bakes each callee's scope-exit `Release`s into
/// its body, so inlining copies them to the return site (the continuation), preserving object
/// lifetimes exactly. Inlining a callee whose value it *returns* moves the transferred `+1` into the
/// call's destination via a plain copy, which is balanced because the callee already skipped
/// releasing the returned value.
///
/// [`ExpandSimpleCtors`] runs *before* [`RcInsertion`] so `o.field = arg` is what RC sees, not a
/// `New` whose args are all treated as sinks. After inlining, [`RcLastUseRepair`] fixes last-use
/// container stores of owned RC values on the fused CFG (inlined `split` temps). Then
/// [`crate::driver`] runs the per-function [`PassManager`].
pub fn optimize_module(mir: &mut Mir, interner: &TypeInterner) {
    optimize_module_opts(mir, interner, true, &mut MirDump::disabled())
}

/// Like [`optimize_module`], but `inline` can be disabled. Debug-info builds turn inlining off so
/// each user function keeps its own body (and thus its own call-stack frame + local variables),
/// which the interactive debugger relies on. Reference-counting insertion + dead-function pruning
/// still run in both modes since they are correctness-relevant, not just optimizations. Every
/// module stage is reported to `dump` under its pass name.
pub fn optimize_module_opts(
    mir: &mut Mir,
    interner: &TypeInterner,
    inline: bool,
    dump: &mut MirDump,
) {
    const MAX_ROUNDS: usize = 8;
    crate::prune_module(mir, interner);
    let _ = ExpandSimpleCtors.run(mir, interner);
    dump.module(ExpandSimpleCtors.name(), mir, interner);
    // Before RC insertion: address-taken functions move their parameter retains to the callee so a
    // funcbox call site can pass at +0 (see `funcbox_abi`).
    let _ = FuncboxAbi.run(mir, interner);
    crate::prune_module(mir, interner);
    dump.module(FuncboxAbi.name(), mir, interner);
    let _ = ParamModes.run(mir, interner);
    dump.module(ParamModes.name(), mir, interner);
    let layouts = mir.layouts.clone();
    let holds = rc::lifetime::held_defs(&mir.intrinsics, &mir.imports);
    let modref = rc::modref::ModRefTable::compute(mir, interner);
    for f in mir.functions.iter_mut().chain(mir.polls.iter_mut()) {
        RcInsertion::run_with_layouts(f, interner, &layouts, &holds, &modref);
    }
    dump.module(MirPass::name(&RcInsertion), mir, interner);
    // Correctness invariant: RC must be inserted (above) *before* any inlining (below), or callee
    // scope-exit releases won't be baked into bodies for inlining to copy. The `rc_inserted` flag
    // makes a future reordering that hoists the inliner above this point fail loudly in dev.
    let _rc_inserted = true;
    debug_assert!(_rc_inserted, "RcInsertion must run before the inliner");
    if inline {
        let _ = Devirt.run(mir, interner);
        dump.module(Devirt.name(), mir, interner);
        let inliner = Inliner;
        for _ in 0..MAX_ROUNDS {
            let changed = inliner.run(mir, interner);
            // Drop callees left with no remaining call sites after inlining (plus their transitively
            // dead callees), then loop: the smaller module may expose more inlining.
            crate::prune_module(mir, interner);
            dump.module(inliner.name(), mir, interner);
            if !changed {
                break;
            }
            let _ = Devirt.run(mir, interner);
            dump.module(Devirt.name(), mir, interner);
        }
    }
    for f in mir.functions.iter_mut().chain(mir.polls.iter_mut()) {
        RcLastUseRepair::run_with_layouts(f, interner, &layouts);
    }
    dump.module(MirPass::name(&RcLastUseRepair), mir, interner);
    let _ = UniqueRegion.run(mir, interner);
    dump.module(UniqueRegion.name(), mir, interner);
    let _ = rc::held::run(mir, interner);
    dump.module(rc::held::STAGE, mir, interner);
    let _ = SroaManaged.run(mir, interner);
    dump.module(SroaManaged.name(), mir, interner);
    let _ = slice_measure::run(mir, interner);
    dump.module(slice_measure::STAGE, mir, interner);
}

/// Runs `pipeline` over every function and `poll_pipeline` over every async poll body, then
/// reports the [`STAGE_FIXPOINT`] snapshot.
pub fn run_function_pipelines(
    mir: &mut Mir,
    interner: &TypeInterner,
    pipeline: &PassManager,
    poll_pipeline: &PassManager,
    dump: &mut MirDump,
) {
    for f in &mut mir.functions {
        pipeline.run_dumped(f, interner, dump);
    }
    for p in &mut mir.polls {
        poll_pipeline.run_dumped(p, interner, dump);
    }
    dump.module(STAGE_FIXPOINT, mir, interner);
}

/// After per-function opts, drop inferred regions whose leave is followed by a still-live
/// ref use (CFG simplify can merge a join with JSON `as_string` / `unwrap` after wrap), then give
/// objects that never outlive their frame stack storage ([`frame_alloc`]). In debug builds of the
/// compiler, the final MIR is then checked by [`crate::verify`].
pub fn run_late_module_passes(mir: &mut Mir, interner: &TypeInterner, dump: &mut MirDump) {
    let _ = unique_region::strip_escaped_regions(mir, interner);
    dump.module(STAGE_LATE, mir, interner);
    let _ = frame_alloc::run(mir, interner);
    dump.module(frame_alloc::STAGE, mir, interner);
    if cfg!(debug_assertions) {
        crate::verify::assert_module(mir, interner);
    }
}
