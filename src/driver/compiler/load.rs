use super::*;

pub(super) struct LoadedProgram<'a> {
    pub acc: ProgramAccumulator<'a>,
    pub graph: dream_sema::module_graph::ModuleGraph<'a>,
    pub native_graph: crate::driver::native_sets::NativeGraph,
    pub cpp_bridge: crate::driver::ffi_shim::CppBridge,
}

/// Where the program's nodes live: generated and synthesized nodes in `bump`, parsed source files
/// in `herd` so they can be parsed in parallel.
#[derive(Default)]
pub(super) struct SourceArenas {
    pub bump: Bump,
    pub herd: bumpalo_herd::Herd,
}

/// The program as generators see it: parsed, prelude merged, attributes validated.
struct FrontEnd<'a> {
    acc: ProgramAccumulator<'a>,
    attributes: dream_abi::attributes::UserAttributes,
    native_graph: crate::driver::native_sets::NativeGraph,
    cpp_bridge: crate::driver::ffi_shim::CppBridge,
}

impl Compiler {
    fn generate_request<'r>(
        &'r self,
        main_file_path: &'r str,
    ) -> crate::driver::generate::GenerateRequest<'r> {
        crate::driver::generate::GenerateRequest {
            config: &self.toolchain_config,
            stage: self.generator_stage,
            entry_file: main_file_path,
            target: self.target.spec(),
            replay_materialized: false,
        }
    }

    fn load_front<'a>(
        &self,
        main_file_path: &String,
        arenas: &'a SourceArenas,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<FrontEnd<'a>, CompileError> {
        let arena = &arenas.bump;
        let mut acc = ProgramAccumulator {
            cfg: self.cfg_symbols(),
            track_build_inputs: self.build_cache.is_some(),
            parse_herd: Some(&arenas.herd),
            ..ProgramAccumulator::default()
        };
        if acc.track_build_inputs
            && let Some(directory) = Path::new(main_file_path).parent()
        {
            for ancestor in directory.ancestors() {
                let manifest = ancestor.join("dream.toml");
                acc.observe_resolution_input(ancestor.join("dream.lock"));
                acc.observe_resolution_input(manifest.clone());
                if manifest.is_file() {
                    break;
                }
            }
        }
        let source_phase = tracing::info_span!("compile_phase", phase = "source_load").entered();
        match &self.virtual_entry {
            Some(source) => {
                // Compiler-made entries (test runners, generator harnesses) call into the files
                // they import, including file-private `@test` functions.
                acc.generated_files.insert(main_file_path.clone());
                crate::driver::source_loader::parse_source_recursive(
                    main_file_path.clone(),
                    source.clone(),
                    &mut acc,
                    arena,
                    diagnostics,
                )?
            }
            None => parse_file_recursive(main_file_path, &mut acc, arena, diagnostics)?,
        }

        if !self.target.spec().capabilities.js_interop {
            for (file, program) in &acc.parsed_files {
                for import in program.imports.iter().filter(|import| import.is_js) {
                    diagnostics.file_path = Some(file.clone());
                    diagnostics.report_error(
                        "jsimport requires a JavaScript host target (--web or --node)".into(),
                        Some(import.module_name.position),
                    );
                }
            }
        }
        if !diagnostics.has_errors() {
            crate::driver::js_imports::prepare(
                arena,
                &mut acc,
                main_file_path,
                self.runtimes
                    .first()
                    .map(|target| target.as_str())
                    .unwrap_or("web"),
                true,
                diagnostics,
            )?;
        }
        let native_graph =
            crate::driver::native_sets::NativeGraph::load(main_file_path, &acc, self.target.spec())
                .map_err(CompileError::Manifest)?;
        #[cfg(feature = "native")]
        if !diagnostics.has_errors() {
            crate::driver::c_imports::prepare(
                &mut acc,
                arena,
                diagnostics,
                main_file_path,
                self.target.spec(),
                crate::driver::c_imports::Build {
                    config: &self.toolchain_config,
                    native: &native_graph,
                    optimize: if self.target.spec().capabilities.linear_memory {
                        self.effective_optimize().unwrap_or(OptLevel::O0)
                    } else {
                        self.native_opt()
                    },
                    debug: self.profile.is_debug(),
                },
            )?;
        }
        #[cfg(not(feature = "native"))]
        crate::driver::c_imports::replay(
            &mut acc,
            arena,
            diagnostics,
            main_file_path,
            self.target.spec(),
        )?;
        drop(source_phase);
        // Opt-in stdlib packages (`import system.io;`, etc.) plus always-on bootstrap
        // (`system.core` / `system.primitives`).
        let prelude_phase = tracing::info_span!("compile_phase", phase = "prelude_parse").entered();
        merge_prelude(&mut acc)?;
        drop(prelude_phase);
        let _attributes_phase =
            tracing::info_span!("compile_phase", phase = "attributes").entered();

        // Validate every attribute in the merged program (unknown names, disallowed placements,
        // wrong argument shapes, duplicates) against the builtin specs and the program's declared
        // `@attribute` types, before generators and semantic analysis read them.
        let attributes = crate::driver::attributes::prepare(arena, &mut acc, diagnostics)?;
        if diagnostics.has_errors() {
            return Err(fail_diagnostics(
                self.render_diagnostics,
                CompileError::Syntax,
                diagnostics,
                &acc.file_contents,
            ));
        }
        crate::driver::native_sets::resolve_bare_c_attrs(&mut acc, &native_graph, diagnostics);
        let cpp_bridge = crate::driver::ffi_shim::expand(
            arena,
            &mut acc,
            &native_graph,
            &attributes,
            diagnostics,
        )?;
        let attributes = crate::driver::attributes::prepare(arena, &mut acc, diagnostics)?;
        Ok(FrontEnd {
            acc,
            attributes,
            native_graph,
            cpp_bridge,
        })
    }

    pub(super) fn load_program<'a>(
        &self,
        main_file_path: &String,
        arenas: &'a SourceArenas,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<LoadedProgram<'a>, CompileError> {
        let arena = &arenas.bump;
        let FrontEnd {
            mut acc,
            attributes,
            native_graph,
            cpp_bridge,
        } = self.load_front(main_file_path, arenas, diagnostics)?;

        let _generators_phase =
            tracing::info_span!("compile_phase", phase = "generators").entered();
        run_generators(
            &self.generate_request(main_file_path),
            arena,
            &mut acc,
            &attributes,
            diagnostics,
        )?;

        crate::driver::attributes::prepare(arena, &mut acc, diagnostics)?;

        // Inherit interface default-method bodies into implementing classes that omit them, by
        // appending synthesized `extend` blocks (must run after class collection so `implements`
        // clauses are all present).
        crate::driver::interface_defaults::generate_interface_default_impls(
            &acc.all_structs,
            &acc.all_interfaces,
            &mut acc.all_extends,
        );

        if diagnostics.has_errors() {
            return Err(fail_diagnostics(
                self.render_diagnostics,
                CompileError::Generator,
                diagnostics,
                &acc.file_contents,
            ));
        }

        Ok(LoadedProgram {
            graph: acc.module_graph(),
            acc,
            native_graph,
            cpp_bridge,
        })
    }

    /// Runs the front end and generator discovery for `main_file_path` and reports what the
    /// generate pass would do, without running any generator.
    pub fn inspect_generators(
        &self,
        main_file_path: &str,
    ) -> Result<crate::driver::generate::GenInspection, CompileError> {
        let arenas = SourceArenas::default();
        let entry = main_file_path.to_string();
        let mut diagnostics = DiagnosticBag::new(Some(entry.clone()));
        let front = self.load_front(&entry, &arenas, &mut diagnostics)?;
        let inspection = crate::driver::generate::inspect(
            &self.generate_request(main_file_path),
            &front.acc,
            &front.attributes,
            &mut diagnostics,
        );
        if diagnostics.has_errors() {
            return Err(fail_diagnostics(
                self.render_diagnostics,
                CompileError::Generator,
                &diagnostics,
                &front.acc.file_contents,
            ));
        }
        Ok(inspection)
    }
}
