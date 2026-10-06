use super::*;

pub(super) struct EmittedModule {
    bytes: Vec<u8>,
    header: String,
    exports: Vec<String>,
    export_functions: Vec<dream_abi::exports::ExportFunction>,
    c_shim: Option<String>,
    live_imports: Vec<(String, String)>,
    threads: bool,
    need: dream_mir::runtime::RuntimeNeed,
}

impl Compiler {
    fn guest_opt(&self) -> OptLevel {
        // LLVM -O0 bloats guest code; the CLI level still controls Binaryen independently.
        match self.optimize {
            None | Some(OptLevel::O0) => OptLevel::O1,
            Some(level) => level,
        }
    }

    pub(super) fn emit_module(
        &self,
        mir: &dream_mir::Mir,
        interner: &dream_types::TypeInterner,
        live_imports: Vec<(String, String)>,
        llvm: Option<&dyn LlvmToolchain>,
    ) -> Result<EmittedModule, CompileError> {
        info!("starting code generation");
        let threads = self.target.spec().capabilities.linear_memory
            && dream_mir::backend::module_needs_threads(mir, interner);
        let need = dream_mir::runtime::runtime_need_from_mir(mir);
        let req = LlvmRuntimeRequest {
            need,
            target: self.target.clone(),
            threads,
            wasm_opt: self.guest_opt(),
        };
        let runtime = llvm
            .ok_or_else(|| CompileError::Toolchain("no LLVM toolchain configured".into()))?
            .runtime_sigs(&req)
            .map_err(CompileError::Toolchain)?;
        let cache = runtime.cache_path.display();
        let stale =
            |e| CompileError::Toolchain(format!("runtime signature cache `{cache}` is stale: {e}"));
        let sigs = dream_mir::backend::llvm::RuntimeSigs::parse(&runtime.text)
            .and_then(|sigs| {
                sigs.validate_target(req.target.spec())?;
                sigs.validate_reference_abi(req.target.spec())?;
                Ok(sigs)
            })
            .map_err(stale)?;
        let mut diagnostics = DiagnosticBag::new(None);
        for (_, export) in &mir.exports {
            if sigs.has_function(export) || mir.functions.iter().any(|f| f.symbol == *export) {
                diagnostics.report_error(
                    format!(
                        "@export symbol '{export}' conflicts with another function or the runtime"
                    ),
                    None,
                );
            }
        }
        if diagnostics.has_errors() {
            return Err(fail_diagnostics(
                CompileError::Semantic,
                &diagnostics,
                &Default::default(),
            ));
        }
        let _phase = tracing::info_span!("compile_phase", phase = "ir_emission").entered();
        let module = dream_mir::backend::llvm::emit_llvm_module(
            mir,
            interner,
            &sigs,
            self.debug && self.target.spec().capabilities.native_entry,
            self.target.clone(),
        )
        .map_err(|error| stale(error.to_string()))?;
        let c_shim = (!module.c_shim.is_empty())
            .then(|| crate::driver::ffi_shim::c_shim::render(&module.c_shim));
        Ok(EmittedModule {
            bytes: module.ir.into_bytes(),
            header: module.header,
            export_functions: module.exports,
            exports: mir
                .exports
                .iter()
                .map(|(_, name)| name.clone())
                .chain(
                    mir.functions
                        .iter()
                        .filter(|f| f.name == dream_mir::abi::ENTRY_FN)
                        .map(|_| dream_mir::abi::ENTRY_FN.to_string()),
                )
                .collect(),
            c_shim,
            live_imports,
            threads,
            need,
        })
    }

    pub(super) fn emit_artifacts(
        &self,
        out_path: &str,
        loaded: &load::LoadedProgram<'_>,
        emitted: EmittedModule,
        layouts: dream_hir::LayoutTable,
        llvm: Option<&dyn LlvmToolchain>,
    ) -> Result<(), CompileError> {
        info!("finished code generation");
        let shim_path = c_shim_path(&Path::new(out_path).with_extension("ll"));
        match &emitted.c_shim {
            Some(src) => fs::write(&shim_path, src)?,
            None => {
                let _ = fs::remove_file(&shim_path);
            }
        }
        let abi_artifacts = emit_wasm_and_abi(
            out_path,
            &loaded.graph.view(),
            &emitted.live_imports,
            &loaded.native_graph,
            &loaded.cpp_bridge,
            &crate::driver::abi::ModuleAbi {
                layouts: &layouts,
                exports: &emitted.exports,
                export_functions: &emitted.export_functions,
                target_triple: &self.target.spec().llvm_triple(),
            },
        )?;
        if !self.target.spec().capabilities.linear_memory {
            fs::write(out_path, &emitted.bytes)?;
            if self.output_kind.is_library() {
                let header = Path::new(out_path).with_extension("h");
                fs::write(&header, &emitted.header)?;
                self.reporter.artifact(&header);
            }
            if !self.opt_ir {
                self.reporter.artifact(Path::new(out_path));
            }
            for p in abi_artifacts {
                self.reporter.artifact(&p);
            }
            return Ok(());
        }
        let wasm_path = std::path::Path::new(out_path).with_extension("wasm");
        let ll_path = std::path::Path::new(out_path).with_extension("ll");
        fs::write(&ll_path, &emitted.bytes)?;
        let opt_ll = self.opt_ir.then(|| ll_path.with_extension("opt.ll"));
        let req = LlvmRuntimeRequest {
            need: emitted.need,
            target: self.target.clone(),
            threads: emitted.threads,
            wasm_opt: self.guest_opt(),
        };
        llvm.ok_or_else(|| CompileError::Internal("no LLVM toolchain configured".into()))?
            .link_wasm(&ll_path, &wasm_path, opt_ll.as_deref(), &req)
            .map_err(CompileError::Toolchain)?;
        self.reporter
            .artifact(opt_ll.as_deref().unwrap_or(&ll_path));
        self.reporter.artifact(&wasm_path);

        // Post-process order matters: wasm-opt first (it drops unknown custom sections), then
        // embed the ABI custom section, then read the final binary once to print `.wat` — so
        // the text always mirrors the shipped bytes.
        if let Some(level) = self.optimize {
            llvm.ok_or_else(|| CompileError::Internal("no LLVM toolchain configured".into()))?
                .optimize_wasm(&wasm_path, level)
                .map_err(CompileError::Toolchain)?;
            debug!("wasm-opt applied at {level:?}: {}", wasm_path.display());
        }

        for p in abi_artifacts {
            self.reporter.artifact(&p);
        }

        crate::driver::abi::embed_abi_in_wasm(out_path)?;

        let wasm_bytes = fs::read(&wasm_path)?;
        let text = dream_mir::backend::print_wasm(&wasm_bytes);
        fs::write(out_path, &text)?;
        self.reporter.artifact(Path::new(out_path));

        // Release builds ship pre-compressed siblings (.gz / .br) for servers with
        // `gzip_static` / `brotli_static` (or CDNs); browsers never compress on their own.
        if self.optimize.is_some() {
            for (path, _) in crate::driver::compress::write_precompressed(&wasm_path) {
                self.reporter.artifact(&path);
            }
        }

        // Opt-in tree-shaken JS hosts (`--runtime --web` / `--runtime --node`); minified on
        // optimizing builds.
        if !self.runtimes.is_empty() {
            let runtime_paths = crate::driver::js_runtime::emit_selective_runtimes(
                out_path,
                &wasm_bytes,
                &self.runtimes,
                self.optimize.is_some(),
            )?;
            for p in runtime_paths {
                self.reporter.artifact(&p);
            }
        }

        Ok(())
    }
}
