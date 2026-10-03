use bumpalo::Bump;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info};

use crate::driver::abi::emit_wasm_and_abi;
use crate::driver::diag_highlight::highlight_dream_line;
use crate::driver::error::CompileError;
use crate::driver::generate::run_generators;
use crate::driver::js_runtime::JsRuntimeTarget;
use crate::driver::prelude::merge_prelude;
use crate::driver::source_loader::{parse_file_recursive, ProgramAccumulator};
use crate::driver::ui::{BuildReporter, SilentReporter};
use crate::driver::wasm_opt::OptLevel;
use dream_abi::attributes::CompileTargets;
use dream_diagnostics::{format_diagnostics, render_with, DiagnosticBag};
use dream_mir::backend::Target;
use dream_sema::analyzer::Analyzer;
use dream_syntax::nodes::ProgramNode;
use dream_syntax::syntax_tree::SyntaxTree;

/// The runtime a module links against: its needed catalog modules, target, and for wasm32 whether
/// the module runs on shared memory and the guest optimization level.
pub struct LlvmRuntimeRequest {
    pub need: dream_mir::runtime::RuntimeNeed,
    pub target: dream_mir::backend::Target,
    pub threads: bool,
    pub wasm_opt: OptLevel,
}

/// The reduced runtime signature table and the cache artifact it came from.
pub struct RuntimeSignatures {
    pub text: String,
    pub cache_path: std::path::PathBuf,
}

/// The pinned LLVM toolchain as the driver sees it. The native execution layer implements it and
/// installs itself by default; the driver stays toolchain-free.
pub trait LlvmToolchain: Send + Sync {
    /// The runtime signature table (`dream_rt.sigs` text) the backend types runtime calls from.
    fn runtime_sigs(&self, req: &LlvmRuntimeRequest) -> Result<RuntimeSignatures, String>;
    /// `.ll` → `.wasm`: whole-program link with the wasm runtime bitcode, `opt`, `llc`, `wasm-ld`.
    /// With `opt_ll`, also writes the optimized whole-program module there as text.
    fn link_wasm(
        &self,
        ll: &Path,
        wasm: &Path,
        opt_ll: Option<&Path>,
        req: &LlvmRuntimeRequest,
    ) -> Result<(), String>;
}

/// Orchestrates the compilation pipeline: source loading (delegated to `source_loader`/`prelude`),
/// semantic analysis, code generation, and artifact emission (delegated to `abi`). Diagnostic
/// rendering is delegated to the `diagnostics` module.
pub struct Compiler {
    toolchain_config: Arc<crate::driver::toolchain::ToolchainConfig>,
    target: Target,
    /// When `true` (the default), codegen emits allocator instrumentation so the
    /// `Debug.live_objects()` / `Debug.total_allocations()` probes report real values, and keeps
    /// every runtime helper in the WAT (skips structural dead-function elimination). Release builds
    /// (`--release` / [`Compiler::with_release`]) turn this off for a trimmed, uninstrumented module.
    debug: bool,
    /// When `true`, the compiler threads source-line info through HIR/MIR so the backend can emit
    /// source-line hooks / line directives for the interactive debugger. Off by default;
    /// enabled via the CLI `-g`/`--debug-info` flag or [`Compiler::with_debug_info`].
    debug_info: bool,
    /// When set, the emitted `.wasm` is post-processed in place with Binaryen's `wasm-opt` at this
    /// level. [`Compiler::with_release`] enables [`OptLevel::RELEASE_DEFAULT`] when no level was
    /// set yet; an explicit [`Compiler::with_optimize`] (or CLI `-O`) overrides that default.
    /// Debug builds leave this `None` unless the caller opts in.
    optimize: Option<OptLevel>,
    /// When `true`, skip the source-generator pass (`@json`, …). Used when compiling
    /// the generator harness itself so nested compiles cannot recurse into generator execution.
    skip_generators: bool,
    /// When non-empty (CLI `--runtime --web` / `--runtime --node`), emit a tree-shaken sibling
    /// `*.{web,node}.runtime.js` for each listed host (both may be set in one compile).
    runtimes: Vec<JsRuntimeTarget>,
    /// Active compile-time runtime target(s) for semantic availability checks. Defaults to
    /// native-only; overridden by `--target` or inferred from `--runtime --web`/`--node`.
    compile_targets: CompileTargets,
    /// Library vs binary; libs reject a primary-file `main`.
    crate_type: dream_sema::analyzer::CrateType,
    /// Progress/artifact sink (silent by default; the CLI installs [`ConsoleReporter`](crate::driver::ui::ConsoleReporter)).
    reporter: Arc<dyn BuildReporter>,
    /// CLI `--emit-mir`: MIR snapshots written to `<out>.mir/<NN>-<pass>.mir`.
    emit_mir: Option<dream_mir::passes::MirDumpSpec>,
    /// When `true`, the unoptimized `.ll` is an intermediate the caller deletes: it is not reported
    /// as an artifact, and wasm32 builds write the optimized module as `<stem>.opt.ll` instead.
    opt_ir: bool,
    llvm: Option<Arc<dyn LlvmToolchain>>,
}

impl Compiler {
    pub fn new(target: Target) -> Self {
        Self::new_with_toolchain_config(
            target,
            Arc::new(crate::driver::toolchain::ToolchainConfig::default()),
        )
    }

    pub fn new_with_toolchain_config(
        target: Target,
        toolchain_config: Arc<crate::driver::toolchain::ToolchainConfig>,
    ) -> Self {
        Self {
            target,
            toolchain_config,
            debug: true,
            debug_info: false,
            optimize: None,
            skip_generators: false,
            runtimes: Vec::new(),
            compile_targets: CompileTargets::native_only(),
            crate_type: dream_sema::analyzer::CrateType::Bin,
            reporter: Arc::new(SilentReporter),
            emit_mir: None,
            opt_ir: false,
            llvm: None,
        }
    }

    pub fn toolchain_config(&self) -> &Arc<crate::driver::toolchain::ToolchainConfig> {
        &self.toolchain_config
    }

    /// The native optimization level these settings build at.
    pub fn native_opt(&self) -> OptLevel {
        OptLevel::from_cli(!self.debug, self.optimize)
    }

    fn toolchain(&self) -> Option<Arc<dyn LlvmToolchain>> {
        if let Some(t) = &self.llvm {
            return Some(t.clone());
        }
        #[cfg(feature = "native")]
        {
            Some(Arc::new(crate::execution::llvm::Toolchain {
                config: self.toolchain_config.clone(),
                opt: self.native_opt(),
                debug: self.debug_info,
            }))
        }
        #[cfg(not(feature = "native"))]
        {
            None
        }
    }

    /// Builder: replace the default LLVM toolchain (the pinned one, with the `native` feature).
    pub fn with_llvm(mut self, toolchain: Arc<dyn LlvmToolchain>) -> Self {
        self.llvm = Some(toolchain);
        self
    }

    /// Builder: receive artifact paths and non-fatal warnings through `reporter` instead of
    /// staying silent. Library users may install their own sink; the CLI uses a console one.
    pub fn with_reporter(mut self, reporter: Arc<dyn BuildReporter>) -> Self {
        self.reporter = reporter;
        self
    }

    /// Builder: skip `@json` / syntax-DSL generators (for compiling generator harnesses).
    pub fn with_skip_generators(mut self, on: bool) -> Self {
        self.skip_generators = on;
        self
    }

    /// Builder: when `on` is `true`, produce a release module — uninstrumented allocator, structural
    /// WAT dead-function elimination (`strip_dead_functions`), and wasm-opt at
    /// [`OptLevel::RELEASE_DEFAULT`] unless a level was already set via [`Compiler::with_optimize`].
    /// When `false` (the default from [`Compiler::new`]), keep allocator probes and the full runtime
    /// (does not clear a previously configured optimize level).
    pub fn with_release(mut self, on: bool) -> Self {
        self.debug = !on;
        if on && self.optimize.is_none() {
            self.optimize = Some(OptLevel::RELEASE_DEFAULT);
        }
        self
    }

    /// Builder: enable source-level debug-info instrumentation (line hooks + source map) for the
    /// interactive debugger.
    pub fn with_debug_info(mut self, on: bool) -> Self {
        self.debug_info = on;
        self
    }

    /// Builder: post-process the emitted `.wasm` with Binaryen's `wasm-opt` at the given level.
    /// `Some(level)` sets/overrides (including the [`OptLevel::RELEASE_DEFAULT`] from
    /// [`Compiler::with_release`]); `None` clears post-processing entirely.
    pub fn with_optimize(mut self, level: Option<OptLevel>) -> Self {
        self.optimize = level;
        self
    }

    /// Builder: emit selective `*.{web,node}.runtime.js` hosts. Empty skips emission (default).
    /// Duplicates are removed while preserving first-seen order (`web` before `node` if both).
    pub fn with_runtimes(mut self, targets: Vec<JsRuntimeTarget>) -> Self {
        let mut seen_web = false;
        let mut seen_node = false;
        let mut out = Vec::new();
        for t in targets {
            match t {
                JsRuntimeTarget::Web if !seen_web => {
                    seen_web = true;
                    out.push(t);
                }
                JsRuntimeTarget::Node if !seen_node => {
                    seen_node = true;
                    out.push(t);
                }
                _ => {}
            }
        }
        // `--release --web` without an explicit `-O` keeps download size (`-Os`); native `--release`
        // stays at [`OptLevel::RELEASE_DEFAULT`] (`-O3`).
        if seen_web && self.optimize == Some(OptLevel::RELEASE_DEFAULT) {
            self.optimize = Some(OptLevel::WEB_RELEASE_DEFAULT);
        }
        self.runtimes = out;
        if !self.runtimes.is_empty() {
            self.compile_targets = CompileTargets {
                native: false,
                node: seen_node,
                web: seen_web,
            };
        }
        self
    }

    /// Builder: set compile-time runtime target(s) explicitly (`--target native|node|web`).
    pub fn with_compile_targets(mut self, targets: CompileTargets) -> Self {
        self.compile_targets = targets;
        self
    }

    /// Builder: `lib` rejects a top-level `main` in the primary file; `bin` is the default.
    pub fn with_crate_type(mut self, crate_type: dream_sema::analyzer::CrateType) -> Self {
        self.crate_type = crate_type;
        self
    }

    /// Builder: dump MIR snapshots selected by `spec` into a `<out>.mir/` directory next to the
    /// output (`None` disables).
    pub fn with_emit_mir(mut self, spec: Option<dream_mir::passes::MirDumpSpec>) -> Self {
        self.emit_mir = spec;
        self
    }

    /// Builder: publish the optimized `<stem>.opt.ll` rather than the unoptimized `.ll`, which the
    /// caller removes once linked.
    pub fn with_opt_ir(mut self, on: bool) -> Self {
        self.opt_ir = on;
        self
    }

    pub fn compile(&self, main_file_path: &String, out_path: &String) -> Result<(), CompileError> {
        info!("starting parsing and multi-file resolution");
        let mut acc = ProgramAccumulator::default();

        let arena = Bump::new();
        let mut diagnostics = DiagnosticBag::new(None);

        parse_file_recursive(main_file_path, &mut acc, &arena, &mut diagnostics)?;

        let native_graph = crate::driver::native_sets::NativeGraph::load(main_file_path, &acc)
            .map_err(CompileError::Manifest)?;
        crate::driver::native_sets::resolve_bare_c_attrs(&mut acc, &native_graph, &mut diagnostics);
        let cpp_bridge =
            crate::driver::cpp_bridge::expand(&arena, &mut acc, &native_graph, &mut diagnostics)?;

        // Opt-in stdlib packages (`import system.net;`, etc.) plus always-on bootstrap
        // (`system.core` / `system.primitives`). `@json` types need `system.json` for derives.
        if program_uses_json_attr(&acc) {
            acc.requested_std_packages.insert("system.json".to_string());
        }
        if program_uses_gpu_shader_attr(&acc) {
            acc.requested_std_packages.insert("system.gpu".to_string());
        }
        merge_prelude(
            &arena,
            &mut acc.all_functions,
            &mut acc.all_structs,
            &mut acc.all_interfaces,
            &mut acc.all_enums,
            &mut acc.all_extends,
            &mut acc.all_globals,
            &mut diagnostics,
            &mut acc.file_contents,
            &mut acc.file_modules,
            &acc.requested_std_packages,
        )?;

        // Validate every attribute in the merged program (unknown names, disallowed placements,
        // wrong argument shapes, duplicates) before anything downstream (the `@json` derive below,
        // then semantic analysis) reads attributes assuming they are well-formed.
        dream_abi::attributes::validate_program_attributes(
            &acc.all_structs,
            &acc.all_interfaces,
            &acc.all_functions,
            &acc.all_enums,
            &acc.all_extends,
            &mut diagnostics,
        );
        if diagnostics.has_errors() {
            return Err(fail_diagnostics(
                CompileError::Syntax,
                &diagnostics,
                &acc.file_contents,
            ));
        }

        // Source generators: `@json` derive and registered `@generator`s (executed `GenContext`
        // bodies). Nested generator compiles set `skip_generators` so this cannot recurse.
        if !self.skip_generators {
            debug_assert!(
                !acc.all_structs.is_empty(),
                "run_generators must run after prelude merge / class collection"
            );
            run_generators(
                &self.toolchain_config,
                &arena,
                &mut acc,
                main_file_path,
                &mut diagnostics,
            )?;
        }

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
                CompileError::Generator,
                &diagnostics,
                &acc.file_contents,
            ));
        }

        let combined_program = ProgramNode::new(
            vec![],
            acc.all_structs,
            acc.all_interfaces,
            acc.all_functions,
            acc.all_enums,
            acc.all_extends,
            acc.all_globals,
        );
        let ast = SyntaxTree::new(combined_program);

        info!("finished parsing");
        info!("starting semantic analysis");

        let file_modules = acc
            .file_modules
            .iter()
            .map(|(file, module)| (std::rc::Rc::from(file.as_str()), module.clone()))
            .collect();
        let mut analyzer = Analyzer::new(&ast, &arena)
            .with_file_modules(file_modules)
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
        let debug_info = self.debug_info;
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
            let gpu = crate::driver::gpu_gen::collect_gpu_shaders(ast.get_root(), &mut diagnostics);
            if diagnostics.has_errors() {
                return Err("generator");
            }

            info!("starting code generation");
            let dream_sema::analyzer::SemanticInfo { hir, .. } = symbol_info;
            let interner = analyzer.interner();
            let mut mir = dream_mir::lower::lower_program(&hir, interner);
            dump_ref.module(dream_mir::passes::STAGE_LOWER, &mir, interner);
            // Whole-module optimization: simple-ctor expand, RC insertion, inlining, last-use RC
            // repair on fused bodies (see `mir::passes::optimize_module`). Per-function only elides pairs.
            // Debug-info builds skip inlining and use a value-preserving per-function pipeline so
            // user variables and per-function call frames survive for the debugger; release builds
            // use the full optimizing pipeline.
            dream_mir::passes::optimize_module_opts(&mut mir, interner, !debug_info, dump_ref);
            let pipeline = if debug_info {
                dream_mir::passes::PassManager::debug_pipeline()
            } else {
                dream_mir::passes::PassManager::release_pipeline()
            };
            let poll_pipeline = if debug_info {
                dream_mir::passes::PassManager::new()
            } else {
                dream_mir::passes::PassManager::async_poll_pipeline()
            };

            dream_mir::passes::run_function_pipelines(
                &mut mir,
                interner,
                &pipeline,
                &poll_pipeline,
                dump_ref,
            );
            dream_mir::passes::run_late_module_passes(&mut mir, interner, dump_ref);
            let live_imports: Vec<(String, String)> = mir
                .imports
                .iter()
                .map(|imp| (imp.module.clone(), imp.field.clone()))
                .collect();
            let linear_memory = target.spec().capabilities.linear_memory;
            if !target.spec().capabilities.c_interop
                && report_wasm_c_imports(
                    ast.get_root(),
                    &live_imports,
                    &cpp_bridge,
                    &mut diagnostics,
                )
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
                        Ok(ir) => ir.into_bytes(),
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
            if !self.opt_ir {
                self.reporter.artifact(Path::new(out_path));
            }
            let abi_artifacts = emit_wasm_and_abi(
                out_path,
                ast.get_root(),
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
            ast.get_root(),
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

impl Compiler {
    /// Replaces `<out>.mir/` with this compile's snapshots (stale files from an earlier dump
    /// would otherwise interleave with the new numbering).
    fn write_mir_dump(
        &self,
        out_path: &str,
        dump: dream_mir::passes::MirDump,
    ) -> Result<(), CompileError> {
        let dir = Path::new(out_path).with_extension("mir");
        if dir.is_dir() {
            fs::remove_dir_all(&dir)?;
        }
        let files = dump.finish();
        if files.is_empty() {
            self.reporter.warning(
                "--emit-mir: the requested pass did not run (or matched no function) in this pipeline",
            );
            return Ok(());
        }
        fs::create_dir_all(&dir)?;
        for f in &files {
            fs::write(dir.join(&f.name), &f.contents)?;
        }
        self.reporter.artifact(&dir);
        Ok(())
    }
}

fn fail_diagnostics(
    ctor: fn(String) -> CompileError,
    diagnostics: &DiagnosticBag,
    file_contents: &std::collections::HashMap<String, String>,
) -> CompileError {
    render_with(diagnostics, file_contents, Some(highlight_dream_line));
    ctor(format_diagnostics(
        diagnostics,
        file_contents,
        false,
        Some(highlight_dream_line),
    ))
}

/// Extracts a human-readable message from a caught panic payload (the `Any` that
/// `std::panic::catch_unwind` hands back), covering the two shapes `panic!`/`internal_error!`
/// actually produce (`&'static str` and `String`).
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "internal compiler error: codegen panicked with a non-string payload".to_string()
    }
}

/// Prints a caught codegen panic the way [`render`] prints ordinary diagnostics, so an internal
/// compiler error looks like the rest of the CLI's output rather than a raw Rust panic dump.
fn render_internal_error(message: &str) {
    eprintln!("error: {}", message);
}

/// Reports every `@c` extern whose import survived MIR pruning on a wasm32 build: native C/C++
/// only links into native binaries, so the call has no host to bind to. True when any was found.
fn report_wasm_c_imports(
    program: &ProgramNode<'_>,
    live_imports: &[(String, String)],
    cpp: &crate::driver::cpp_bridge::CppBridge,
    diagnostics: &mut DiagnosticBag,
) -> bool {
    let live: std::collections::BTreeSet<(&str, &str)> = live_imports
        .iter()
        .filter(|(m, _)| m.starts_with("c/"))
        .map(|(m, f)| (m.as_str(), f.as_str()))
        .collect();
    if live.is_empty() {
        return false;
    }
    let methods = program.structs.iter().flat_map(|s| s.methods.iter());
    let extends = program.extends.iter().flat_map(|e| e.methods.iter());
    let mut reported = std::collections::BTreeSet::new();
    for f in program.functions.iter().chain(methods).chain(extends) {
        if !f.is_extern || !dream_abi::attributes::has_c_attr(&f.attributes) {
            continue;
        }
        let (module, field) =
            dream_abi::attributes::extern_import_target(&f.attributes, &f.name.text);
        if !live.contains(&(module.as_str(), field.as_str())) || !reported.insert((module, field)) {
            continue;
        }
        let (what, at, file) = match cpp.origin(&f.name.text) {
            Some(o) => (o.what.as_str(), o.at, Some(o.file.to_string())),
            None if crate::driver::cpp_bridge::is_generated(&f.name.text) => continue,
            None => (
                f.name.text.as_str(),
                f.name.position,
                f.file_path.as_deref().map(str::to_string),
            ),
        };
        diagnostics.report(dream_diagnostics::Diagnostic::new(
            format!(
                "'{what}' is a native C/C++ import and cannot be called from a wasm32 build; \
                 bind the browser/Node equivalent with `@js` (or guard the call with `@native`)"
            ),
            Some(at),
            file,
        ));
    }
    true
}

/// True when any collected user type carries `@json` (derived converters need `system.json`).
fn program_uses_json_attr(acc: &ProgramAccumulator<'_>) -> bool {
    acc.all_structs
        .iter()
        .any(|s| s.attributes.iter().any(|a| a.name.text == "json"))
        || acc
            .all_enums
            .iter()
            .any(|e| e.attributes.iter().any(|a| a.name.text == "json"))
}

/// True when any top-level function carries `@compute` / `@vertex` / `@fragment` (needs `system.gpu`).
fn program_uses_gpu_shader_attr(acc: &ProgramAccumulator<'_>) -> bool {
    acc.all_functions
        .iter()
        .any(|f| dream_abi::attributes::is_gpu_shader_attr(&f.attributes))
}
