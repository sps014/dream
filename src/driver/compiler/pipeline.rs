use super::*;

impl Compiler {
    pub fn compile(&self, main_file_path: &String, out_path: &String) -> Result<(), CompileError> {
        info!("starting parsing and multi-file resolution");

        let arena = Bump::new();
        let mut diagnostics = DiagnosticBag::new(None);

        let load::LoadedProgram {
            acc,
            graph,
            native_graph,
            cpp_bridge,
        } = self.load_program(main_file_path, &arena, &mut diagnostics)?;

        let program = graph.view();

        info!("finished parsing");
        info!("starting semantic analysis");

        let mut analyzer = Analyzer::new(&graph, &arena)
            .with_aliased_imports(acc.aliased_imports)
            .with_crate_type(self.crate_type, Some(main_file_path.clone()))
            .with_compile_targets(self.compile_targets)
            .with_target_layout(dream_hir::TargetLayout {
                ptr_size: self.target.spec().ptr_size,
                ptr_align: self.target.spec().ptr_align,
            });
        analyzer.set_debug_info(self.debug_info);
        let file_contents = &acc.file_contents;
        // Analyzer panics (`internal_error!`) share this ICE net with codegen: `SemanticInfo`
        // borrows the analyzer, so analysis and emission run in one `catch_unwind`.
        let _quiet = crate::driver::quiet_panic::QuietPanics::new();
        let target = &self.target;
        let debug = self.debug;
        let mut dump = match &self.emit_mir {
            Some(spec) => dream_mir::passes::MirDump::new(spec.clone()),
            None => dream_mir::passes::MirDump::disabled(),
        };
        let dump_ref = &mut dump;
        let llvm = self.toolchain();
        let llvm_ref = llvm.as_ref();
        // The guest never compiles below -O1 (`-O0` only selects the Binaryen level): naive -O0
        // codegen bloats both the toolchain's own work and the module; -O1 is near-free.
        let guest_opt = match self.optimize {
            None | Some(OptLevel::O0) => OptLevel::O1,
            Some(level) => level,
        };
        let mut llvm_err: Option<String> = None;
        let llvm_err_ref = &mut llvm_err;
        let mut c_shim: Option<String> = None;
        let c_shim_ref = &mut c_shim;
        let pipeline_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let symbol_info = match analyzer.analyze(&mut diagnostics) {
                Ok(info) => info,
                Err(_) => {
                    return Err("semantic");
                }
            };
            info!("finished semantic analysis");
            if !diagnostics.diagnostics.is_empty() {
                render_with(&diagnostics, file_contents, Some(highlight_dream_line));
            }
            let gpu = crate::driver::gpu_gen::collect_gpu_shaders(&program, &mut diagnostics);
            if diagnostics.has_errors() {
                return Err("generator");
            }

            info!("starting code generation");
            let dream_sema::analyzer::SemanticInfo { hir, .. } = symbol_info;
            let interner = analyzer.interner();
            let mir = self.lower_and_optimize(&hir, interner, dump_ref);
            let live_imports: Vec<(String, String)> = mir
                .imports
                .iter()
                .map(|imp| (imp.module.clone(), imp.field.clone()))
                .collect();
            let linear_memory = target.spec().capabilities.linear_memory;
            if !target.spec().capabilities.c_interop
                && report_wasm_c_imports(&program, &live_imports, &cpp_bridge, &mut diagnostics)
            {
                return Err("semantic");
            }
            let threads = linear_memory && dream_mir::backend::module_needs_threads(&mir, interner);
            let need = dream_mir::runtime::runtime_need_from_mir(&mir);
            let req = LlvmRuntimeRequest {
                need,
                target: target.clone(),
                threads,
                wasm_opt: guest_opt,
            };
            let runtime = llvm_ref
                .ok_or_else(|| "no LLVM toolchain configured".to_string())
                .and_then(|t| t.runtime_sigs(&req));
            let bytes: Vec<u8> = match runtime {
                Ok(runtime) => {
                    let cache = runtime.cache_path.display();
                    let sigs = dream_mir::backend::llvm::RuntimeSigs::parse(&runtime.text)
                        .and_then(|sigs| {
                            sigs.validate_target(req.target.spec())?;
                            sigs.validate_reference_abi(req.target.spec())?;
                            Ok(sigs)
                        })
                        .map_err(|e| format!("runtime signature cache `{cache}` is stale: {e}"));
                    let sigs = match sigs {
                        Ok(sigs) => sigs,
                        Err(e) => {
                            *llvm_err_ref = Some(e);
                            return Err("llvm");
                        }
                    };
                    match dream_mir::backend::llvm::emit_llvm_module(
                        &mir,
                        interner,
                        &sigs,
                        debug && target.spec().capabilities.native_entry,
                        req.target.clone(),
                    ) {
                        Ok(module) => {
                            *c_shim_ref = (!module.c_shim.is_empty())
                                .then(|| crate::driver::ffi_shim::c_shim::render(&module.c_shim));
                            module.ir.into_bytes()
                        }
                        Err(e) => {
                            *llvm_err_ref =
                                Some(format!("runtime signature cache `{cache}` is stale: {e}"));
                            return Err("llvm");
                        }
                    }
                }
                Err(e) => {
                    *llvm_err_ref = Some(e);
                    return Err("llvm");
                }
            };
            Ok((bytes, live_imports, threads, need, gpu, mir.layouts.clone()))
        }));

        if self.emit_mir.is_some() {
            self.write_mir_dump(out_path, dump)?;
        }

        let (bytes, live_imports, threads, need, gpu, layouts) = match pipeline_result {
            Ok(Ok(tuple)) => tuple,
            Ok(Err("semantic")) => {
                return Err(fail_diagnostics(
                    CompileError::Semantic,
                    &diagnostics,
                    &acc.file_contents,
                ));
            }
            Ok(Err("llvm")) => {
                return Err(CompileError::Toolchain(
                    llvm_err.unwrap_or_else(|| "LLVM runtime unavailable".into()),
                ));
            }
            Ok(Err(_)) => {
                return Err(fail_diagnostics(
                    CompileError::Generator,
                    &diagnostics,
                    &acc.file_contents,
                ));
            }
            Err(panic_payload) => {
                let message = panic_message(&panic_payload);
                render_internal_error(&message);
                return Err(CompileError::Internal(message));
            }
        };

        info!("finished code generation");
        if !self.target.spec().capabilities.linear_memory {
            fs::write(out_path, &bytes)?;
            let shim_path = c_shim_path(Path::new(out_path));
            match &c_shim {
                Some(src) => fs::write(&shim_path, src)?,
                None => {
                    let _ = fs::remove_file(&shim_path);
                }
            }
            if !self.opt_ir {
                self.reporter.artifact(Path::new(out_path));
            }
            let abi_artifacts = emit_wasm_and_abi(
                out_path,
                &program,
                &gpu,
                &live_imports,
                &native_graph,
                &cpp_bridge,
                &layouts,
            )?;
            for p in abi_artifacts {
                self.reporter.artifact(&p);
            }
            return Ok(());
        }
        let wasm_path = std::path::Path::new(out_path).with_extension("wasm");
        let ll_path = std::path::Path::new(out_path).with_extension("ll");
        fs::write(&ll_path, &bytes)?;
        let opt_ll = self.opt_ir.then(|| ll_path.with_extension("opt.ll"));
        let req = LlvmRuntimeRequest {
            need,
            target: self.target.clone(),
            threads,
            wasm_opt: guest_opt,
        };
        llvm.as_ref()
            .ok_or_else(|| CompileError::Internal("no LLVM toolchain configured".into()))?
            .link_wasm(&ll_path, &wasm_path, opt_ll.as_deref(), &req)
            .map_err(CompileError::Internal)?;
        self.reporter
            .artifact(opt_ll.as_deref().unwrap_or(&ll_path));
        self.reporter.artifact(&wasm_path);

        // Post-process order matters: wasm-opt first (it drops unknown custom sections), then
        // embed the ABI custom section, then read the final binary once to print `.wat` — so
        // the text always mirrors the shipped bytes.
        if let Some(level) = self.optimize {
            // Non-fatal: the unoptimized `.wasm` is already valid output.
            match crate::driver::wasm_opt::optimize_wasm_file(&wasm_path, level) {
                Ok(()) => debug!("wasm-opt applied at {level:?}: {}", wasm_path.display()),
                Err(e) => self.reporter.warning(&format!(
                    "could not optimize {} with wasm-opt: {}",
                    wasm_path.display(),
                    e
                )),
            }
        }

        // Sibling `.abi.json` for JS/`dream.js` interop, plus `.wgsl` when GPU kernels were emitted.
        let abi_artifacts = emit_wasm_and_abi(
            out_path,
            &program,
            &gpu,
            &live_imports,
            &native_graph,
            &cpp_bridge,
            &layouts,
        )?;
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
                &live_imports,
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
