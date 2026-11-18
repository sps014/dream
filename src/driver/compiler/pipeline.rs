use super::*;

#[path = "analyze.rs"]
mod analyze;
#[path = "body_cache.rs"]
mod body_cache;
#[path = "cache.rs"]
mod cache;
#[path = "emit.rs"]
mod emit;
#[path = "lower.rs"]
mod lower;

pub use cache::{BuildOutcome, BuildStamp};

impl Compiler {
    pub fn compile(
        &self,
        main_file_path: &String,
        out_path: &str,
    ) -> Result<BuildOutcome, CompileError> {
        crate::driver::cache_budget::enforce_shared(self.toolchain_config());
        if self.output_kind.is_library() && self.target.spec().capabilities.linear_memory {
            return Err(CompileError::Manifest(
                "staticlib and dylib require a native target".into(),
            ));
        }
        let early = {
            let _phase = tracing::info_span!("compile_phase", phase = "cache_lookup").entered();
            self.early_build_lookup(main_file_path, out_path)
        };
        if let Some(artifacts) = early {
            info!("reusing cached build before parsing");
            return Ok(BuildOutcome::Cached(artifacts));
        }
        info!("starting parsing and multi-file resolution");
        let arena = Bump::new();
        let mut diagnostics = DiagnosticBag::new(None);
        let loaded = {
            let _phase = tracing::info_span!("compile_phase", phase = "parse").entered();
            self.load_program(main_file_path, &arena, &mut diagnostics)?
        };
        info!("finished parsing");
        let modules = self
            .build_cache
            .is_some()
            .then(|| loaded.graph.module_hashes());
        let (stamp, cached) = {
            let _phase = tracing::info_span!("compile_phase", phase = "cache_lookup").entered();
            let stamp = self.build_cache.as_deref().and_then(|link_key| {
                self.build_stamp(
                    link_key,
                    &loaded,
                    modules.as_deref()?,
                    main_file_path,
                    out_path,
                )
            });
            let cached = stamp.as_ref().and_then(BuildStamp::lookup);
            (stamp, cached)
        };
        if let Some(artifacts) = cached {
            info!("reusing cached build");
            return Ok(BuildOutcome::Cached(artifacts));
        }

        let mut analyzer = self.prepare_analyzer(&loaded, &arena);
        let mut body_cache = modules.as_ref().and_then(|modules| {
            body_cache::BodyCache::new(
                self.toolchain_config(),
                &loaded,
                modules,
                (self.target.spec(), self.crate_type, self.debug_info),
            )
        });
        if let Some(cache) = &mut body_cache {
            let (hits, typed) = cache.load();
            tracing::info!(modules = hits.len(), "reusing syntax-only body checks");
            analyzer.set_cached_body_checks(hits);
            analyzer.set_cached_typed_bodies(typed);
        }
        let mut dump = match &self.emit_mir {
            Some(spec) => dream_mir::passes::MirDump::new(spec.clone()),
            None => dream_mir::passes::MirDump::disabled(),
        };
        let llvm = self.toolchain();
        let quiet = crate::driver::quiet_panic::QuietPanics::new();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Analysis and lowering recurse with the tree. A fresh stack keeps that recursion
            // off the (often small) main thread of an installed binary or the language server.
            stacker::maybe_grow(64 * 1024, 16 * 1024 * 1024, || {
                let analyzed = self.analyze_graph(&mut analyzer, &loaded, &mut diagnostics)?;
                if let Some(cache) = &body_cache {
                    let typed = analyzer.take_typed_bodies();
                    tracing::info!(
                        functions = analyzer.reused_typed_bodies(),
                        "reused typed HIR"
                    );
                    cache.store(analyzer.body_checks_with_diagnostics(), &typed);
                }
                let interner = analyzer.interner();
                let mut mir = self.lower_hir(&analyzed.hir, interner, &mut dump);
                if self.output_kind.is_library() {
                    library::relativize_sources(&mut mir, main_file_path)?;
                }
                let mir = self.optimize_mir(mir, interner, &mut dump);
                let live_imports: Vec<_> = mir
                    .imports
                    .iter()
                    .map(|imp| (imp.module.clone(), imp.field.clone()))
                    .collect();
                if let Some(environment) = self.target.spec().execution_environment.component() {
                    if !mir.exports.is_empty()
                        || self.crate_type == dream_sema::analyzer::CrateType::Lib
                    {
                        diagnostics.report_error("standalone WASI emits command components; exported libraries require a WIT world".into(), None);
                    }
                    for import in &mir.imports {
                        let supported = import.module.starts_with("c/")
                            || (import.module == dream_abi::js_abi::HOST_MODULE
                                && (import.component
                                    || dream_mir::runtime::modules::runtime_module_host_field(
                                        &import.field,
                                    )));
                        if !supported {
                            diagnostics.report_error(
                                format!(
                                    "capability '{}.{}' is not available on {environment}",
                                    import.module, import.field
                                ),
                                None,
                            );
                        }
                    }
                    if diagnostics.has_errors() {
                        return Err(fail_diagnostics(
                            self.render_diagnostics,
                            CompileError::Semantic,
                            &diagnostics,
                            &loaded.acc.file_contents,
                        ));
                    }
                }
                if self.target.spec().capabilities.linear_memory
                    && report_wasm_c_imports(
                        &loaded.graph.view(),
                        &live_imports,
                        &loaded.cpp_bridge,
                        &loaded.native_graph,
                        &mut diagnostics,
                    )
                {
                    return Err(fail_diagnostics(
                        self.render_diagnostics,
                        CompileError::Semantic,
                        &diagnostics,
                        &loaded.acc.file_contents,
                    ));
                }
                let emitted =
                    self.emit_module(&mir, interner, live_imports, &loaded, llvm.as_deref())?;
                Ok((emitted, mir.layouts))
            })
        }));
        drop(quiet);
        if self.emit_mir.is_some() {
            self.write_mir_dump(out_path, dump)?;
        }
        let (emitted, layouts) = match result {
            Ok(result) => result?,
            Err(payload) => {
                let message = panic_message(&payload);
                render_internal_error(&message);
                return Err(CompileError::Internal(message));
            }
        };
        {
            let _phase = tracing::info_span!("compile_phase", phase = "artifacts").entered();
            self.emit_artifacts(out_path, &loaded, &emitted, layouts, llvm.as_deref())?;
        }
        self.publish_generated_artifacts(out_path, &loaded.acc)?;
        Ok(BuildOutcome::Built {
            stamp: stamp.map(|stamp| stamp.with_warnings(&diagnostics, &loaded.acc.file_contents)),
            codegen_units: emitted.codegen_units,
        })
    }
}
