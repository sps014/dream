use bumpalo::Bump;
use std::fs;
use tracing::{info, warn};

use crate::driver::abi::emit_abi_sidecars;
use crate::driver::error::CompileError;
use crate::driver::generate::run_generators;
use crate::driver::js_runtime::JsRuntimeTarget;
use crate::driver::prelude::merge_prelude;
use crate::driver::source_loader::{parse_file_recursive, ProgramAccumulator};
use crate::driver::wasm_opt::OptLevel;
use dream_abi::attributes::CompileTargets;
use dream_diagnostics::{render, DiagnosticBag};
use dream_sema::analyzer::Analyzer;
use dream_syntax::nodes::ProgramNode;
use dream_syntax::syntax_tree::SyntaxTree;

#[derive(Clone, Copy)]
pub enum Target {
    /// Host LLVM triple (desktop `dream run`).
    Native,
    /// `wasm32-unknown-unknown` via LLVM (`--web` / playground).
    Wasm,
}

fn llvm_opts_for(target: Target) -> dream_llvm::CodegenOptions {
    let mut opts = dream_llvm::CodegenOptions::default();
    if matches!(target, Target::Wasm) {
        opts.triple =
            dream_llvm::Triple::parse("wasm32-unknown-unknown").expect("wasm32-unknown-unknown");
    }
    opts
}

/// Orchestrates the compilation pipeline: source loading (delegated to `source_loader`/`prelude`),
/// semantic analysis, code generation, and artifact emission (delegated to `abi`). Diagnostic
/// rendering is delegated to the `diagnostics` module.
pub struct Compiler {
    /// When `true`, the compiler threads source-line info through HIR/MIR and the backend emits
    /// source-line hooks + a `.dbg.json` source map for the interactive debugger. Off by default;
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
    /// When `true` (default), write sibling `.abi.json` for JS/`dream.js` interop. GPU
    /// kernels are still written whenever the program emits GPU (native `dream-rt` loads them).
    emit_abi: bool,
    /// Library vs binary; libs reject a primary-file `main`.
    crate_type: dream_sema::analyzer::CrateType,
    /// When set, emit LLVM IR and invoke clang instead of WAT.
            llvm: dream_llvm::CodegenOptions,
}

impl Compiler {
    pub fn new(target: Target) -> Self {
        Self {
            debug_info: false,
            optimize: None,
            skip_generators: false,
            runtimes: Vec::new(),
            compile_targets: CompileTargets::native_only(),
            emit_abi: true,
            crate_type: dream_sema::analyzer::CrateType::Bin,
            llvm: llvm_opts_for(target),
        }
    }

    /// Builder: skip `@json` / syntax-DSL generators (for compiling generator harnesses).
    pub fn with_skip_generators(mut self, on: bool) -> Self {
        self.skip_generators = on;
        self
    }

    /// Builder: release LLVM (`-O3`, thin LTO) plus wasm-opt on wasm32 unless `-O` already set.
    pub fn with_release(mut self, on: bool) -> Self {
        if on {
            self.llvm = self.llvm.clone().release();
            if self.optimize.is_none() {
                self.optimize = Some(OptLevel::RELEASE_DEFAULT);
            }
        }
        self
    }

    /// Builder: enable source-level debug-info instrumentation (line hooks + source map) for the
    /// interactive debugger.
    pub fn with_debug_info(mut self, on: bool) -> Self {
        self.debug_info = on;
        self.llvm.debug_info = on;
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

    /// Builder: write `.abi.json` next to the module (needed for browser/Node + `dream.js`).
    /// Disable for native `run` / `debug-adapter` where wasmtime links hosts in Rust.
    pub fn with_emit_abi(mut self, on: bool) -> Self {
        self.emit_abi = on;
        self
    }

    /// Builder: `lib` rejects a top-level `main` in the primary file; `bin` is the default.
    pub fn with_crate_type(mut self, crate_type: dream_sema::analyzer::CrateType) -> Self {
        self.crate_type = crate_type;
        self
    }

    /// Builder: overlay LLVM flags (`--triple`, `--lto`, ASan, …).
    pub fn with_llvm(mut self, opts: dream_llvm::CodegenOptions) -> Self {
        self.llvm = opts;
        self
    }

    pub fn compile(&self, main_file_path: &String, out_path: &String) -> Result<(), CompileError> {
        info!("starting parsing and multi-file resolution");
        let mut acc = ProgramAccumulator::default();

        let arena = Bump::new();
        let mut diagnostics = DiagnosticBag::new(None);

        parse_file_recursive(main_file_path, &mut acc, &arena, &mut diagnostics)?;

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
            render(&diagnostics, &acc.file_contents);
            return Err(CompileError::Syntax);
        }

        // Source generators: `@json` derive and registered `@generator`s (executed `GenContext`
        // bodies). Nested generator compiles set `skip_generators` so this cannot recurse.
        if !self.skip_generators {
            debug_assert!(
                !acc.all_structs.is_empty(),
                "run_generators must run after prelude merge / class collection"
            );
            run_generators(&arena, &mut acc, main_file_path, &mut diagnostics)?;
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
            render(&diagnostics, &acc.file_contents);
            return Err(CompileError::Generator);
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

        let file_modules: std::collections::HashMap<std::rc::Rc<str>, std::rc::Rc<str>> = acc
            .file_modules
            .iter()
            .map(|(file, module)| (std::rc::Rc::from(file.as_str()), module.clone()))
            .collect();
        let mut analyzer = Analyzer::new(&ast, &arena)
            .with_file_modules(file_modules)
            .with_aliased_imports(acc.aliased_imports)
            .with_crate_type(self.crate_type, Some(main_file_path.clone()))
            .with_compile_targets(self.compile_targets);
        analyzer.set_debug_info(self.debug_info);
        // `analyze` reports each error into the bag and returns a typed failure once any error was
        // recorded, short-circuiting before code generation runs on a poisoned program.
        let symbol_info = match analyzer.analyze(&mut diagnostics) {
            Ok(info) => info,
            Err(_) => {
                render(&diagnostics, &acc.file_contents);
                return Err(CompileError::Semantic);
            }
        };

        info!("finished semantic analysis");

        // Validate GPU shaders (unsupported stmts / stage rules) before MIR so failures don't leave a
        // half-written `.wat` behind a generator error.
        let gpu = crate::driver::gpu_gen::collect_gpu_shaders(ast.get_root(), &mut diagnostics);
        if diagnostics.has_errors() {
            render(&diagnostics, &acc.file_contents);
            return Err(CompileError::Generator);
        }

        info!("starting code generation");

        // Lower the analyzer-emitted HIR to MIR, optimize, and emit a self-contained module.
        // Destructuring moves the owned `hir` out and drops `symbol_info`'s borrowing references,
        // releasing the `&mut analyzer` borrow so the shared interner can be read (the HIR references
        // its `TypeId`s, so both must come from this same analyzer instance).
        let dream_sema::analyzer::SemanticInfo { hir, .. } = symbol_info;
        let interner = analyzer.interner();
        let debug_info = self.debug_info;

        // Codegen (MIR lowering/optimization/emission) treats certain lookups - a type's layout, an
        // interned string, a function-table slot - as compiler invariants rather than user errors: a
        // miss means the analyzer and codegen disagree about a well-typed program, i.e. a compiler
        // bug (see `crate::internal_error!`). Catching the resulting panic here turns that into a
        // clean, typed `CompileError::Internal` instead of an unwinding panic with a raw backtrace
        // reaching the user.
        // Suppress the default panic hook's raw "thread 'main' panicked at ..." dump for the
        // duration of this call: the panic is already a well-formed internal-error message (see
        // `render_internal_error` below), and we don't want a Rust backtrace header confusing users
        // who never expect to see a stack trace from a compiler CLI.
        // `take_hook`/`set_hook` operate on process-global state, so concurrent `compile()` calls
        // (e.g. the e2e corpus running on a rayon pool) racing on this swap can permanently clobber
        // the hook with another thread's no-op — the mutex below serializes the swap+restore so each
        // call's `take_hook` always observes the real previous hook.
        static HOOK_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let hook_guard = HOOK_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let codegen_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut mir = dream_mir::lower::lower_program(&hir, interner);
            // Whole-module optimization: tree-shaking + reference-counting insertion + function
            // inlining (see `mir::passes::optimize_module`). RC is inserted there, before inlining,
            // so callee destruction stays deterministic; the per-function pipeline below only cleans
            // up the merged bodies.
            // Debug-info builds skip inlining and use a value-preserving per-function pipeline so
            // user variables and per-function call frames survive for the debugger; release builds
            // use the full optimizing pipeline.
            let llvm = {
                let mut llvm = self.llvm.clone();
                llvm.debug_info = debug_info;
                llvm
            };
            dream_mir::passes::optimize_module_opts(&mut mir, interner, !debug_info);
            let pipeline = if debug_info {
                dream_mir::passes::PassManager::debug_pipeline()
            } else {
                dream_mir::passes::PassManager::default_pipeline()
            };
            for f in &mut mir.functions {
                pipeline.run(f, interner);
            }
            let live_imports: Vec<(String, String)> = mir
                .imports
                .iter()
                .map(|imp| (imp.module.clone(), imp.field.clone()))
                .collect();
            let ir = dream_llvm::emit_ir(&mir, interner, &llvm);
            let dbg = if debug_info {
                Some(dream_llvm::debug_map_json(&mir, interner))
            } else {
                None
            };
            (ir, live_imports, llvm, dbg)
        }));
        std::panic::set_hook(previous_hook);
        drop(hook_guard);

        let (ir, live_imports, opts, dbg) = codegen_result.map_err(|panic_payload| {
            let message = panic_message(&panic_payload);
            render_internal_error(&message);
            CompileError::Internal(message)
        })?;

        info!("finished code generation");
        let ll_path = std::path::Path::new(out_path).with_extension("ll");
        fs::write(&ll_path, &ir)?;
        info!("created file: {}", ll_path.display());
        let bin = if opts.triple.is_wasm() {
            std::path::Path::new(out_path).with_extension("wasm")
        } else if opts.link_shared {
            std::path::Path::new(out_path).with_extension(dream_llvm::shared_lib_ext())
        } else {
            std::path::Path::new(out_path).with_extension("out")
        };
        dream_llvm::compile_ir(&ir, &bin, &opts)
            .map_err(|e| CompileError::Backend(e.to_string()))?;
        info!("created file: {}", bin.display());
        if let Some(dbg) = dbg {
            let dbg_path = std::path::Path::new(out_path).with_extension("dbg.json");
            fs::write(&dbg_path, dbg)?;
            info!("created file: {}", dbg_path.display());
        }

        emit_abi_sidecars(
            out_path,
            ast.get_root(),
            &gpu,
            &live_imports,
            self.emit_abi,
        )?;
        if opts.triple.is_wasm() && self.emit_abi {
            crate::driver::abi::embed_abi_in_wasm(out_path)?;
        }

        if !self.runtimes.is_empty() {
            crate::driver::js_runtime::emit_selective_runtimes(
                out_path,
                &live_imports,
                &self.runtimes,
            )?;
        }

        if let Some(level) = self.optimize {
            if opts.triple.is_wasm() {
                match crate::driver::wasm_opt::optimize_wasm_file(&bin, level) {
                    Ok(()) => info!("optimized file with wasm-opt: {}", bin.display()),
                    Err(e) => warn!("could not run wasm-opt on {}: {}", bin.display(), e),
                }
            }
        }

        Ok(())
    }
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
