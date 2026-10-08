use super::*;

impl Compiler {
    pub(super) fn optimize_mir(
        &self,
        mut mir: dream_mir::Mir,
        interner: &dream_types::TypeInterner,
        dump: &mut dream_mir::passes::MirDump,
    ) -> dream_mir::Mir {
        mir.profile = self.profile();
        {
            let _phase = tracing::info_span!("compile_phase", phase = "module_passes").entered();
            if self.profile().is_debug() {
                dream_mir::passes::prepare_debug_module(&mut mir, interner, dump);
            } else {
                dream_mir::passes::optimize_module_opts(
                    &mut mir,
                    interner,
                    !self.debug_info && !self.output_kind.is_library(),
                    dump,
                );
            }
        }
        let pipeline = if self.profile().is_debug() {
            dream_mir::passes::PassManager::debug_pipeline(self.debug_info)
        } else {
            dream_mir::passes::PassManager::release_pipeline()
        };
        let poll_pipeline = if self.profile().is_debug() {
            dream_mir::passes::PassManager::new()
        } else {
            dream_mir::passes::PassManager::async_poll_pipeline()
        };

        {
            let _phase =
                tracing::info_span!("compile_phase", phase = "per_function_passes").entered();
            dream_mir::passes::run_function_pipelines(
                &mut mir,
                interner,
                &pipeline,
                &poll_pipeline,
                dump,
            );
        }
        {
            let _phase =
                tracing::info_span!("compile_phase", phase = "late_module_passes").entered();
            if !self.profile().is_debug() {
                dream_mir::passes::run_late_module_passes(&mut mir, interner, dump);
            }
            if self.profile().is_debug() || dream_mir::verify::enabled() {
                dream_mir::verify::assert_module(&mir, interner);
            }
        }
        mir
    }
}
