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
use dream_sema::module_graph::ProgramView;

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
    output_kind: crate::driver::output::OutputKind,
    llvm: Option<Arc<dyn LlvmToolchain>>,
    /// Link-stage options folded into the build key; `None` disables the build cache.
    build_cache: Option<String>,
}

mod pipeline;
pub use pipeline::{BuildOutcome, BuildStamp};
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
            output_kind: if target.spec().capabilities.linear_memory {
                crate::driver::output::OutputKind::Wasm
            } else {
                crate::driver::output::OutputKind::Executable
            },
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
            build_cache: None,
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

    /// Library output selects a unit without a process entry point.
    pub fn with_output_kind(mut self, kind: crate::driver::output::OutputKind) -> Self {
        self.output_kind = kind;
        if kind.is_library() {
            self.crate_type = dream_sema::analyzer::CrateType::Lib;
        }
        self
    }

    pub fn with_crate_type(mut self, crate_type: dream_sema::analyzer::CrateType) -> Self {
        self.crate_type = crate_type;
        self
    }

    /// Builder: dump MIR snapshots selected by `spec` into a `<out>.mir/` directory next to the
    /// output (`None` disables).
    /// Builder: reuse an earlier build's artifacts when every input, including the caller's
    /// link-stage options in `link_key`, is unchanged. The caller records fresh artifacts through
    /// the returned [`BuildStamp`] after its own post-processing succeeds.
    pub fn with_build_cache(mut self, link_key: Option<String>) -> Self {
        self.build_cache = link_key;
        self
    }

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

/// The generated `@c` shim source next to a native `.ll`; absent when the program calls no C.
pub fn c_shim_path(ll: &Path) -> std::path::PathBuf {
    ll.with_extension("cshim.c")
}

mod diagnostics;
use diagnostics::{fail_diagnostics, panic_message, render_internal_error};

/// Reports every `@c` extern whose import survived MIR pruning on a wasm32 build: native C/C++
/// only links into native binaries, so the call has no host to bind to. True when any was found.
fn report_wasm_c_imports(
    program: &ProgramView<'_>,
    live_imports: &[(String, String)],
    cpp: &crate::driver::ffi_shim::CppBridge,
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
    for f in program
        .functions
        .iter()
        .copied()
        .chain(methods)
        .chain(extends)
    {
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
            None if crate::driver::ffi_shim::is_generated(&f.name.text) => continue,
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

mod library;
mod load;
mod optimize;
