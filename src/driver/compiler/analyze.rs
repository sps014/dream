use super::*;

pub(super) struct AnalyzedProgram {
    pub hir: dream_hir::Hir,
}

impl Compiler {
    pub(super) fn prepare_analyzer<'a>(
        &self,
        loaded: &'a load::LoadedProgram<'a>,
        arena: &'a Bump,
    ) -> Analyzer<'a> {
        let mut analyzer = Analyzer::new(&loaded.graph, arena)
            .with_aliased_imports(loaded.acc.aliased_imports.clone())
            .with_crate_type(self.crate_type)
            .with_compile_targets(self.compile_targets)
            .with_target_layout(dream_hir::TargetLayout {
                ptr_size: self.target.spec().ptr_size,
                ptr_align: self.target.spec().ptr_align,
            });
        analyzer.set_debug_info(self.debug_info);
        analyzer
    }

    pub(super) fn analyze_graph(
        &self,
        analyzer: &mut Analyzer<'_>,
        loaded: &load::LoadedProgram<'_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<AnalyzedProgram, CompileError> {
        let _phase = tracing::info_span!("compile_phase", phase = "sema").entered();
        info!("starting semantic analysis");
        let hir = match analyzer.analyze(diagnostics) {
            Ok(info) => info.hir,
            Err(_) => {
                return Err(fail_diagnostics(
                    CompileError::Semantic,
                    diagnostics,
                    &loaded.acc.file_contents,
                ));
            }
        };
        // Poison HIR must never reach lowering, even if an analyzer reports an error but returns Ok.
        if diagnostics.has_errors() {
            return Err(fail_diagnostics(
                CompileError::Semantic,
                diagnostics,
                &loaded.acc.file_contents,
            ));
        }
        info!("finished semantic analysis");
        if !diagnostics.diagnostics.is_empty() {
            render_with(
                diagnostics,
                &loaded.acc.file_contents,
                Some(highlight_dream_line),
            );
        }
        Ok(AnalyzedProgram { hir })
    }
}
