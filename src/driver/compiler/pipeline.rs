use super::*;

#[path = "analyze.rs"]
mod analyze;
#[path = "emit.rs"]
mod emit;
#[path = "lower.rs"]
mod lower;

impl Compiler {
    pub fn compile(&self, main_file_path: &String, out_path: &str) -> Result<(), CompileError> {
        info!("starting parsing and multi-file resolution");
        let arena = Bump::new();
        let mut diagnostics = DiagnosticBag::new(None);
        let loaded = self.load_program(main_file_path, &arena, &mut diagnostics)?;
        info!("finished parsing");

        let mut analyzer = self.prepare_analyzer(&loaded, main_file_path, &arena);
        let mut dump = match &self.emit_mir {
            Some(spec) => dream_mir::passes::MirDump::new(spec.clone()),
            None => dream_mir::passes::MirDump::disabled(),
        };
        let llvm = self.toolchain();
        let _quiet = crate::driver::quiet_panic::QuietPanics::new();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let analyzed = self.analyze_graph(&mut analyzer, &loaded, &mut diagnostics)?;
            let interner = analyzer.interner();
            let mir = self.lower_hir(&analyzed.hir, interner, &mut dump);
            let mir = self.optimize_mir(mir, interner, &mut dump);
            let live_imports: Vec<_> = mir
                .imports
                .iter()
                .map(|imp| (imp.module.clone(), imp.field.clone()))
                .collect();
            if !self.target.spec().capabilities.c_interop
                && report_wasm_c_imports(
                    &loaded.graph.view(),
                    &live_imports,
                    &loaded.cpp_bridge,
                    &mut diagnostics,
                )
            {
                return Err(fail_diagnostics(
                    CompileError::Semantic,
                    &diagnostics,
                    &loaded.acc.file_contents,
                ));
            }
            let emitted = self.emit_module(&mir, interner, live_imports, llvm.as_deref())?;
            Ok((emitted, analyzed.gpu, mir.layouts))
        }));
        if self.emit_mir.is_some() {
            self.write_mir_dump(out_path, dump)?;
        }
        let (emitted, gpu, layouts) = match result {
            Ok(result) => result?,
            Err(payload) => {
                let message = panic_message(&payload);
                render_internal_error(&message);
                return Err(CompileError::Internal(message));
            }
        };
        self.emit_artifacts(out_path, &loaded, emitted, gpu, layouts, llvm.as_deref())
    }
}
