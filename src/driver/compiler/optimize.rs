use super::*;

impl Compiler {
    pub(super) fn optimize_mir(
        &self,
        mut mir: dream_mir::Mir,
        interner: &dream_types::TypeInterner,
        dump: &mut dream_mir::passes::MirDump,
    ) -> dream_mir::Mir {
        // Whole-module optimization: simple-ctor expand, RC insertion, inlining, last-use RC
        // repair on fused bodies (see `mir::passes::optimize_module`). Per-function only elides pairs.
        // Debug-info builds skip inlining and use a value-preserving per-function pipeline so
        // user variables and per-function call frames survive for the debugger; release builds
        // use the full optimizing pipeline.
        dream_mir::passes::optimize_module_opts(
            &mut mir,
            interner,
            !self.debug_info && !self.output_kind.is_library(),
            dump,
        );
        let pipeline = if self.debug_info {
            dream_mir::passes::PassManager::debug_pipeline()
        } else {
            dream_mir::passes::PassManager::release_pipeline()
        };
        let poll_pipeline = if self.debug_info {
            dream_mir::passes::PassManager::new()
        } else {
            dream_mir::passes::PassManager::async_poll_pipeline()
        };

        dream_mir::passes::run_function_pipelines(
            &mut mir,
            interner,
            &pipeline,
            &poll_pipeline,
            dump,
        );
        dream_mir::passes::run_late_module_passes(&mut mir, interner, dump);
        mir
    }
}
