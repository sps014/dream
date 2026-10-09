//! Module transformations invalidate function caches by ending their scope.

use super::*;

/// Whole-module optimization: reference-counting insertion, then aggressive tree-shaking interleaved
/// with function inlining, run to a fixpoint.
///
/// Crucially, `RcInsertion` runs *before* inlining. Dream has deterministic, reference-counted
/// destruction, so a local reference's lifetime must end at the point its owning function returns —
/// not at the caller's scope exit. Inserting RC first bakes each callee's scope-exit `Release`s into
/// its body, so inlining copies them to the return site (the continuation), preserving object
/// lifetimes exactly. Inlining a callee whose value it *returns* moves the transferred `+1` into the
/// call's destination via an explicit `Move`, which is balanced because the callee already skipped
/// releasing the returned value.
///
/// [`ExpandSimpleCtors`] runs *before* [`RcInsertion`] so `o.field = arg` is what RC sees, not a
/// `New` whose args are all treated as sinks. Then
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
    optimize_module_rounds(mir, interner, inline, dump, 8);
}

pub(super) fn optimize_module_rounds(
    mir: &mut Mir,
    interner: &TypeInterner,
    inline: bool,
    dump: &mut MirDump,
    max_rounds: usize,
) {
    prepare_ownership(mir, interner, dump);
    // Correctness invariant: RC must be inserted (above) *before* any inlining (below), or callee
    // scope-exit releases won't be baked into bodies for inlining to copy. The `rc_inserted` flag
    // makes a future reordering that hoists the inliner above this point fail loudly in dev.
    let _rc_inserted = true;
    debug_assert!(_rc_inserted, "RcInsertion must run before the inliner");
    if inline {
        let _ = Devirt.run(mir, interner);
        dump.module(Devirt.name(), mir, interner);
        let _ = inline::recursive::run(mir, interner);
        dump.module("recursive-inline", mir, interner);
        let inliner = Inliner;
        for round in 0..max_rounds {
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
            if round + 1 == max_rounds {
                limits::reached(limits::Limit::Inline, max_rounds, None);
            }
        }
    }
    let _ = UniqueRegion.run(mir, interner);
    dump.module(UniqueRegion.name(), mir, interner);
    let _ = rc::held::run(mir, interner);
    dump.module(rc::held::STAGE, mir, interner);
    let _ = SroaManaged.run(mir, interner);
    dump.module(SroaManaged.name(), mir, interner);
    let _ = slice_measure::run(mir, interner);
    dump.module(slice_measure::STAGE, mir, interner);
}

/// Ownership lowering is mandatory even when every optional optimization is disabled.
pub fn prepare_ownership(mir: &mut Mir, interner: &TypeInterner, dump: &mut MirDump) {
    crate::prune_module(mir, interner);
    let _ = ExpandSimpleCtors.run(mir, interner);
    dump.module(ExpandSimpleCtors.name(), mir, interner);
    // Before RC insertion: address-taken functions move their parameter retains to the callee so a
    // funcbox call site can pass at +0 (see `funcbox_abi`).
    let _ = FuncboxAbi.run(mir, interner);
    crate::prune_module(mir, interner);
    dump.module(FuncboxAbi.name(), mir, interner);
    let _ = param_modes::run(mir, interner);
    dump.module(param_modes::STAGE, mir, interner);
    ownership_args::run(mir, interner);
    dump.module(ownership_args::STAGE, mir, interner);
    let layouts = mir.layouts.clone();
    let holds = rc::lifetime::held_defs(&mir.intrinsics, &mir.imports);
    let modref = rc::modref::ModRefTable::compute(mir, interner);
    for f in mir.functions.iter_mut().chain(mir.polls.iter_mut()) {
        RcInsertion::run_with_layouts(f, interner, &layouts, &holds, &modref);
    }
    dump.module(MirPass::name(&RcInsertion), mir, interner);
    if crate::verify::enabled() {
        crate::verify::assert_inserted_tokens(mir, interner);
    }
}

pub fn prepare_debug_module(mir: &mut Mir, interner: &TypeInterner, dump: &mut MirDump) {
    prepare_ownership(mir, interner, dump);
    let _ = value_borrow::run(mir, interner);
    crate::verify::assert_inserted_tokens(mir, interner);
    crate::verify::assert_module(mir, interner);
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
        pipeline.run_layouts_dumped(f, interner, &mir.layouts, dump);
    }
    for p in &mut mir.polls {
        poll_pipeline.run_layouts_dumped(p, interner, &mir.layouts, dump);
    }
    dump.module(STAGE_FIXPOINT, mir, interner);
}

/// After per-function opts, give objects that never outlive their frame stack storage
/// ([`frame_alloc`]). Invalid ownership or allocation regions are compiler bugs; final MIR is then
/// checked by [`crate::verify`] in debug builds of the compiler, or when `DREAM_VERIFY_MIR=1`.
pub fn run_late_module_passes(mir: &mut Mir, interner: &TypeInterner, dump: &mut MirDump) {
    // Scalar replacement and CFG cleanup expose container owners that the earlier
    // post-inline analysis could not prove live across an interface call.
    let _ = rc::held::run(mir, interner);
    dump.module(rc::held::STAGE, mir, interner);
    let _ = value_borrow::run(mir, interner);
    dump.module(value_borrow::STAGE, mir, interner);
    let panics = value_borrow::panic_defs(mir);
    for f in &mut mir.functions {
        if !borrowed_fields::run(f, interner, &panics) {
            continue;
        }
        let mut analyses = FunctionAnalyses::default();
        GlobalProp.transform(f, interner, &mir.layouts, &mut analyses);
        Sccp.transform(f, interner, &mir.layouts, &mut analyses);
        SimplifyCfg.transform(f, interner, &mir.layouts, &mut analyses);
        Dce.transform(f, interner, &mir.layouts, &mut analyses);
        analyses.invalidate();
        StrCursor.transform(f, interner, &mir.layouts, &mut analyses);
    }
    dump.module("borrowed-fields", mir, interner);
    loop_fields::run(mir, interner);
    dump.module("loop-fields", mir, interner);
    let _ = frame_alloc::run(mir, interner);
    dump.module(frame_alloc::STAGE, mir, interner);
    construction::run(mir, interner);
    dump.module("construction", mir, interner);
    if crate::verify::enabled() {
        crate::verify::assert_module(mir, interner);
    }
}
