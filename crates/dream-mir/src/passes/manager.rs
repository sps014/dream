//! Function pipelines own their analyses for every pass and fixpoint round.

use super::*;

/// Runs a configured pipeline of passes to a fixpoint over each function.
pub struct PassManager {
    pub(super) passes: Vec<Box<dyn MirPass>>,
    pub(super) max_iterations: usize,
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
        pm.add(ReleaseSink);
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

    /// The per-function pipeline release builds run: [`Self::default_pipeline`] without the MIR
    /// `v128` autovectorizer, so LLVM's loop vectorizer sees scalar loops.
    pub fn release_pipeline() -> Self {
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
        pm.add(ReleaseSink);
        pm.add(StrCursor);
        debug_assert!(pm.passes.iter().all(|p| p.name() != "autovec"));
        pm
    }

    /// A pipeline safe for `mir.polls` (async coroutine bodies).
    /// Omits `RcElision` (its ownership facts do not model a suspension handing the frame to the
    /// scheduler between `Await` and its resume block) and `Tco` (tail calls are invalid in
    /// coroutines). `SimplifyCfg` never retargets `Await` resume edges, and `CopyConstProp`
    /// resets at every block, so neither moves a value or a binding across a suspension.
    pub fn async_poll_pipeline() -> Self {
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
        pm.add(Dce);
        pm.add(HopElision);
        pm.add(StrCursor);
        pm
    }

    /// One bounded cleanup round; debug information preserves source locals and call frames.
    pub fn debug_pipeline(debug_info: bool) -> Self {
        let mut pm = Self::new();
        pm.max_iterations = 1;
        if !debug_info {
            pm.add(CopyConstProp);
            pm.add(ConstFold);
            pm.add(Dce);
        }
        pm.add(SimplifyCfg);
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
        self.run_layouts_dumped(func, interner, &dream_hir::LayoutTable::default(), dump);
    }

    pub fn run_layouts_dumped(
        &self,
        func: &mut MirFunction,
        interner: &TypeInterner,
        layouts: &dream_hir::LayoutTable,
        dump: &mut MirDump,
    ) {
        let _function = tracing::info_span!("function_passes", function = %func.symbol).entered();
        let mut analyses = FunctionAnalyses::default();
        let mut revision = 0;
        // A successful pass may need another round. Only a no-change result is reusable, and
        // any later transformation invalidates it through the function revision.
        let mut unchanged_at = vec![None; self.passes.len()];
        for iteration in 0..self.max_iterations {
            let mut changed = false;
            for (index, pass) in self.passes.iter().enumerate() {
                if unchanged_at[index] == Some(revision) {
                    tracing::debug!(
                        event = "mir-pass-skipped",
                        pass = pass.name(),
                        iteration,
                        reason = "unchanged-input"
                    );
                    if dump.is_active() {
                        dump.function_pass(pass.as_ref(), iteration, false, func, interner);
                    }
                    continue;
                }
                let metrics = tracing::enabled!(tracing::Level::DEBUG).then(|| {
                    (
                        std::time::Instant::now(),
                        func.locals.len(),
                        func.blocks.len(),
                        func.blocks.iter().map(|b| b.stmts.len()).sum::<usize>(),
                    )
                });
                let pass_changed = {
                    let _pass =
                        tracing::info_span!("mir_pass", pass = pass.name(), iteration).entered();
                    analyses.run_pass(pass.as_ref(), func, interner, layouts)
                };
                if let Some((start, locals_before, blocks_before, statements_before)) = metrics {
                    tracing::debug!(
                        event = "mir-pass-result",
                        pass = pass.name(),
                        iteration,
                        elapsed_ns = start.elapsed().as_nanos() as u64,
                        changed = pass_changed,
                        locals_before,
                        locals_after = func.locals.len(),
                        blocks_before,
                        blocks_after = func.blocks.len(),
                        statements_before,
                        statements_after = func.blocks.iter().map(|b| b.stmts.len()).sum::<usize>()
                    );
                }
                if dump.is_active() {
                    dump.function_pass(pass.as_ref(), iteration, pass_changed, func, interner);
                }
                if pass_changed {
                    revision += 1;
                    unchanged_at[index] = None;
                } else {
                    unchanged_at[index] = Some(revision);
                }
                changed |= pass_changed;
            }
            if !changed {
                break;
            }
            if iteration + 1 == self.max_iterations {
                limits::reached(limits::Limit::Function, self.max_iterations, Some(func));
            }
        }
    }
}

impl Default for PassManager {
    fn default() -> Self {
        PassManager::new()
    }
}
