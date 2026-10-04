//! Native outputs share runtime linking and optimization; their ABI manifests supply the
//! public roots before the output kind selects an executable, archive or shared library.

use super::c_shim::shim_bitcode;
use super::icon;
use super::runtime::llvm_runtime;
use super::tools::{resolve_llvm, LlvmTools};
use crate::driver::output::OutputKind;
use crate::driver::wasi::run_captured;
use crate::driver::wasm_opt::OptLevel;
use crate::execution::native::bundle::{link_runtime, stage_runtime};
use crate::execution::native::c_link::{
    cc_link_flags, read_c_libs_from_abi, search_roots_for_artifact,
};
use crate::execution::native::native_c::{compile_sets, read_c_sources_from_abi, NativeObjects};
use crate::execution::native::pgo::{clear_raw_profiles, llvm_pgo};
use crate::execution::native::{
    cc, host_library_dir, native_bin_fresh, read_host_capabilities, Pgo,
};
use dream_abi::c_abi::EMBED_EXPORTS;
#[path = "library.rs"]
mod library;
#[path = "build_policy.rs"]
mod policy;
use dream_mir::runtime::runtime_need_from_module_text;
use policy::{cpu_args, dead_strip_args, section_args};
pub(super) use policy::{llc_level, pipeline};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

/// `opt`'s PGO pipeline kind and its profile file (see `pgo::llvm_pgo`).
type PgoPipeline = Option<(&'static str, PathBuf)>;

/// Runs `opt` over the linked module and returns the optimized bitcode path.
fn optimize_linked(
    tools: &LlvmTools,
    linked: &Path,
    level: (OptLevel, bool),
    pgo: &PgoPipeline,
    exports: &[String],
    spec: &dream_abi::target::TargetSpec,
) -> Result<PathBuf, String> {
    let (opt, debug) = level;
    let out = linked.with_extension("opt.bc");
    let mut cmd = tools.command("opt");
    cmd.arg(format!("-passes={}", pipeline(opt, debug)))
        .args(cpu_args(opt, debug, spec))
        .arg(public_api_list(exports));
    if !debug {
        // COFF CodeView records llc's output path even for runtime-only debug units.
        cmd.arg("-strip-debug");
    }
    if let Some((kind, file)) = pgo {
        cmd.arg(format!("-pgo-kind={kind}"))
            .arg(format!("-profile-file={}", file.display()));
    }
    cmd.arg(linked).arg("-o").arg(&out);
    run_captured(&mut cmd, "opt")?;
    Ok(out)
}

/// Explicit module exports, the embedding API, and native-source runtime anchors.
fn public_api_list(exports: &[String]) -> String {
    let mut list = EMBED_EXPORTS.to_vec();
    list.extend(
        exports
            .iter()
            .map(String::as_str)
            .filter(|e| !EMBED_EXPORTS.contains(e)),
    );
    format!("-internalize-public-api-list={}", list.join(","))
}

/// Links `ll_path` with the extra bitcode/IR `modules` (runtime first), then optimizes.
fn link_and_optimize(
    tools: &LlvmTools,
    ll_path: &Path,
    modules: &[&Path],
    level: (OptLevel, bool),
    pgo: &PgoPipeline,
    exports: &[String],
    spec: &dream_abi::target::TargetSpec,
) -> Result<PathBuf, String> {
    let linked = ll_path.with_extension("linked.bc");
    let mut link = tools.command("llvm-link");
    link.arg(ll_path).args(modules).arg("-o").arg(&linked);
    run_captured(&mut link, &format!("llvm-link ({})", ll_path.display()))?;
    let optimized = optimize_linked(tools, &linked, level, pgo, exports, spec);
    let _ = std::fs::remove_file(&linked);
    optimized
}

fn run_llc(
    tools: &LlvmTools,
    input: &Path,
    opt: OptLevel,
    debug: bool,
    filetype: &str,
    out: &Path,
    spec: &dream_abi::target::TargetSpec,
) -> Result<(), String> {
    let mut llc = tools.command("llc");
    llc.arg(llc_level(opt, debug))
        .args(section_args(spec))
        .args(cpu_args(opt, debug, spec))
        .arg(format!("-filetype={filetype}"))
        .arg("-relocation-model=pic")
        .arg(input)
        .arg("-o")
        .arg(out);
    run_captured(&mut llc, "llc")
}

/// Disassembles bitcode `bc` to textual IR at `out`.
pub(super) fn write_ir(tools: &LlvmTools, bc: &Path, out: &Path) -> Result<(), String> {
    let mut dis = tools.command("llvm-dis");
    dis.arg(bc).arg("-o").arg(out);
    run_captured(&mut dis, "llvm-dis")?;
    // llvm-dis writes its input path as ModuleID, which otherwise leaks the build directory.
    let text = std::fs::read_to_string(out).map_err(|e| e.to_string())?;
    let text = text
        .lines()
        .filter(|line| !line.starts_with("; ModuleID = "))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(out, format!("{text}\n")).map_err(|e| e.to_string())
}

/// `--emit-llvm`: the optimized whole-program module as `<stem>.opt.ll` and its assembly as
/// `<stem>.s`, next to the `.ll`.
pub fn emit_llvm_artifacts(
    config: &std::sync::Arc<crate::driver::toolchain::ToolchainConfig>,
    target: &dream_abi::target::TargetSpec,
    ll_path: &Path,
    opt: OptLevel,
    debug: bool,
    icon: Option<&Path>,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let tools = resolve_llvm(config)?;
    let src = std::fs::read_to_string(ll_path)?;
    let rt = llvm_runtime(
        &tools,
        target,
        opt,
        runtime_need_from_module_text(&src),
        debug,
    )?;
    let icon_ll = match icon {
        Some(icon) => Some(icon::write_icon_module(
            ll_path,
            &icon::read_png(icon)?,
            &src,
        )?),
        None => None,
    };
    let shim = shim_bitcode(&tools, target, ll_path)?;
    let optimized = link_and_optimize(
        &tools,
        ll_path,
        &[Some(rt.bc.as_path()), shim.as_deref(), icon_ll.as_deref()]
            .iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>(),
        (opt, debug),
        &None,
        &library::exports(&ll_path.with_extension("abi.json"))?,
        target,
    );
    for p in icon_ll.iter().chain(&shim) {
        let _ = std::fs::remove_file(p);
    }
    let optimized = optimized?;
    let opt_ll = ll_path.with_extension("opt.ll");
    write_ir(&tools, &optimized, &opt_ll)?;
    let asm = ll_path.with_extension("s");
    run_llc(&tools, &optimized, opt, debug, "asm", &asm, target)?;
    let _ = std::fs::remove_file(&optimized);
    Ok(vec![opt_ll, asm])
}

/// Links `ll_path` into `<stem>.bin`. With `opt_ll`, also writes the optimized whole-program module
/// there as text. `icon` is a PNG compiled in as the app icon.
pub struct NativeBuildOptions<'a> {
    pub target: dream_abi::target::TargetSpec,
    pub opt_ll: Option<&'a Path>,
    pub opt: OptLevel,
    pub debug: bool,
    pub pgo: &'a Pgo,
    pub icon: Option<&'a Path>,
    pub relocatable: bool,
    pub output_kind: OutputKind,
}

pub fn compile_llvm(
    config: &std::sync::Arc<crate::driver::toolchain::ToolchainConfig>,
    ll_path: &Path,
    options: NativeBuildOptions<'_>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let NativeBuildOptions {
        target: spec,
        opt_ll,
        opt,
        debug,
        pgo,
        icon,
        relocatable,
        output_kind,
    } = options;
    if !spec.can_link_on_host() && *pgo != Pgo::Off {
        return Err("cross-target PGO requires running the profile on its target".into());
    }
    let tools = resolve_llvm(config)?;
    if output_kind == OutputKind::Wasm {
        return Err("native linking cannot emit wasm".into());
    }
    if output_kind.is_library() && (*pgo != Pgo::Off || icon.is_some()) {
        return Err("native libraries do not support PGO or app icons".into());
    }
    let bin = output_kind.artifact_path(ll_path, &spec);
    let abi_path = ll_path.with_extension("abi.json");
    let capabilities = read_host_capabilities(ll_path)?;
    let dir = host_library_dir(config, &capabilities, &spec).ok_or(
        "native capability libraries not found next to the dream binary. \
         Build with cargo build --workspace, or set DREAM_HOME or DREAM_BIN to the installed toolchain.",
    ).map_err(|error: &str| format!("{error} Target {} requires capability libraries in {}", spec.triple, config.targets.join(spec.triple.to_string()).join("lib").display()))?;
    crate::execution::native::capability_abi::validate(&dir, &capabilities, &spec)?;
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(bin.with_extension("lock"))?;
    lock_file.lock()?;
    let bundled = if relocatable {
        Some(stage_runtime(
            &dir,
            bin.parent().unwrap_or_else(|| Path::new(".")),
            &capabilities,
            &spec,
        )?)
    } else {
        None
    };
    let src = std::fs::read_to_string(ll_path)?;
    let need = runtime_need_from_module_text(&src);
    let rt = llvm_runtime(&tools, &spec, opt, need, debug)?;
    let profile = llvm_pgo(pgo, &bin, || tools.optional_tool("llvm-profdata"))?;
    let icon_png = icon.map(icon::read_png).transpose()?;
    let c_sources = read_c_sources_from_abi(&abi_path);
    if !c_sources.is_empty() && *pgo != Pgo::Off {
        return Err(PGO_NATIVE_SOURCES.into());
    }
    let native = if c_sources.is_empty() {
        NativeObjects::default()
    } else {
        let cache = ll_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("native-c");
        compile_sets(
            config,
            &cc::resolve_target_cc(config, &spec)?,
            &spec,
            &c_sources,
            &cache,
            debug,
        )?
    };
    let stamp_path = bin.with_extension("flags");
    let stamp = format!(
        "native-c-shim-v4\n{}\n{}\n{}\n{}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}",
        pipeline(opt, debug),
        debug,
        llc_level(opt, debug),
        tools.bin.display(),
        rt.archive,
        profile,
        icon_png.as_deref().map(icon::fingerprint),
        native.objects,
        native.link_args,
        (relocatable, output_kind),
        capabilities,
        section_args(&spec),
        dead_strip_args(&spec)
    );
    let driver = cc::resolve_target_cc(config, &spec)?;
    let mut resolved_inputs = vec![driver.path().to_path_buf()];
    if icon_png.is_some() && spec.is_windows() {
        resolved_inputs.push(PathBuf::from(icon::resource_command(&tools)?.get_program()));
    }
    for name in ["llc", "opt", "llvm-link", "llvm-ar"] {
        resolved_inputs.push(tools.tool(name));
    }
    for capability in &capabilities {
        resolved_inputs.push(dir.join(capability.library_name(&spec)));
        if spec.is_windows() {
            resolved_inputs.push(dir.join(capability.import_library_name(&spec)));
        }
    }
    let stamp = format!(
        "{stamp}\n{}\n{spec:?}\n{driver:?}\n{:?}\n{}",
        config.fingerprint(),
        driver
            .cc_command(config, &spec)?
            .get_args()
            .collect::<Vec<_>>(),
        crate::driver::rt_stamp::fingerprint(resolved_inputs)
    );
    let input = match (pgo, &profile) {
        (Pgo::Use(_), Some((_, p))) => Some(p.as_path()),
        _ => None,
    };
    if native_bin_fresh(&bin, ll_path, &rt.bc, input)
        && native.objects.iter().all(|o| !newer_than(o, &bin))
        && opt_ll.is_none_or(Path::exists)
        && (output_kind != OutputKind::Staticlib || ll_path.with_extension("link.json").is_file())
        && std::fs::read_to_string(&stamp_path).is_ok_and(|s| s == stamp)
    {
        return Ok(bin);
    }
    if *pgo == Pgo::Generate {
        clear_raw_profiles(&bin);
    }

    let icon_ll = match &icon_png {
        Some(png) => Some(icon::write_icon_module(ll_path, png, &src)?),
        None => None,
    };
    let mut exports = native.runtime_exports.clone();
    exports.extend(library::exports(&abi_path)?);
    let shim = shim_bitcode(&tools, &spec, ll_path)?;
    let optimized = link_and_optimize(
        &tools,
        ll_path,
        &[Some(rt.bc.as_path()), shim.as_deref(), icon_ll.as_deref()]
            .iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>(),
        (opt, debug),
        &profile,
        &exports,
        &spec,
    );
    for p in icon_ll.iter().chain(&shim) {
        let _ = std::fs::remove_file(p);
    }
    let optimized = optimized?;
    if let Some(out) = opt_ll {
        write_ir(&tools, &optimized, out)?;
    }
    let obj = ll_path.with_extension("o");
    run_llc(&tools, &optimized, opt, debug, "obj", &obj, &spec)?;

    if output_kind == OutputKind::Staticlib {
        library::archive(&tools, &bin, &obj, &native.objects, rt.archive.as_deref())?;
        let mut flags = Vec::new();
        let mut host = std::process::Command::new("cc");
        link_runtime(&mut host, &dir, None, &capabilities, &spec);
        flags.extend(host.get_args().map(|a| a.to_string_lossy().into_owned()));
        if !spec.is_windows() {
            flags.extend(["-lm".to_string(), "-lpthread".to_string()]);
        }
        let libs = read_c_libs_from_abi(&abi_path);
        flags.extend(cc_link_flags(
            config,
            &libs,
            &search_roots_for_artifact(config, ll_path),
            &spec,
        ));
        flags.extend(native.link_args.clone());
        if let Err(error) = library::link_metadata(ll_path, &flags) {
            let _ = std::fs::remove_file(&bin);
            return Err(error);
        }
        std::fs::write(&stamp_path, stamp)?;
        let _ = std::fs::remove_file(&optimized);
        let _ = std::fs::remove_file(&obj);
        return Ok(bin);
    }
    let mut lcmd = if *pgo == Pgo::Generate {
        let mut c = std::process::Command::new(cc::resolve_system_cc(config).ok_or(PGO_NEEDS_CC)?);
        c.arg(&obj).args(profile_link_args(&tools)?);
        c
    } else {
        let cc = cc::resolve_target_cc(config, &spec)?;
        let mut c = if native.needs_cxx && spec.is_msvc() {
            cc.cxx_command(config, &spec)?
        } else {
            cc.cc_command(config, &spec)?
        };
        c.arg(&obj);
        c
    };
    lcmd.args(&native.objects);
    if let Some(version) = spec.min_os {
        lcmd.arg(format!(
            "-m{}-version-min={}.{}.{}",
            if spec.is_ios() {
                if spec.triple.to_string().ends_with("-sim") {
                    "ios-simulator"
                } else {
                    "iphoneos"
                }
            } else {
                "macosx"
            },
            version.major,
            version.minor,
            version.patch
        ));
    }
    let export_file = if output_kind == OutputKind::Dylib {
        Some(library::shared_flags(
            &mut lcmd,
            ll_path,
            &bin,
            &library::exports(&abi_path)?,
            &spec,
        )?)
    } else {
        None
    };
    lcmd.args(dead_strip_args(&spec));
    if let Some(a) = &rt.archive {
        lcmd.arg(a);
    }
    if let Some(png) = &icon_png.as_ref().filter(|_| spec.is_windows()) {
        lcmd.arg(icon::windows_resource(&tools, ll_path, png)?);
    }
    if !spec.is_windows() {
        lcmd.args(["-lm", "-lpthread"]);
    }
    link_runtime(&mut lcmd, &dir, bundled.as_deref(), &capabilities, &spec);
    let c_libs = read_c_libs_from_abi(&abi_path);
    if !c_libs.is_empty() {
        let roots = search_roots_for_artifact(config, ll_path);
        lcmd.args(cc_link_flags(config, &c_libs, &roots, &spec));
    }
    lcmd.args(&native.link_args);
    lcmd.arg("-o").arg(&bin);
    if let Err(e) = run_captured(&mut lcmd, &format!("cc link ({})", obj.display())) {
        let _ = std::fs::remove_file(&bin);
        return Err(e.into());
    }
    if let Some(path) = export_file {
        let _ = std::fs::remove_file(path);
    }
    std::fs::write(&stamp_path, stamp)?;
    let _ = std::fs::remove_file(&optimized);
    if !debug {
        let _ = std::fs::remove_file(&obj);
    }
    Ok(bin)
}

/// PGO links with the system compiler, whose C++ standard library may differ from the one the
/// package's native sources were compiled against.
const PGO_NATIVE_SOURCES: &str =
    "--profile is not supported for programs with `native/` C/C++ sources yet: profile builds \
     link with the system compiler, whose C/C++ runtime may not match the Zig-built objects";

fn newer_than(a: &Path, b: &Path) -> bool {
    let t = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    match (t(a), t(b)) {
        (Some(a), Some(b)) => a > b,
        _ => true,
    }
}

/// zig's linker lays out the `__llvm_prf_*` sections so the profile runtime writes corrupt
/// counters, so instrumented binaries link with the platform linker.
const PGO_NEEDS_CC: &str =
    "--profile needs a system C compiler (cc or clang on PATH, or CC): the Zig toolchain \
     cannot link profile-instrumented binaries";

/// What `clang -fprofile-generate` adds at link time: compiler-rt's profile runtime, kept alive by
/// its registration symbol (Linux) or with the counter sections page-aligned as its Mach-O
/// writer expects (macOS).
fn profile_link_args(tools: &LlvmTools) -> Result<Vec<String>, String> {
    let rt = super::bundle::clang_rt(tools, super::bundle::ClangRt::Profile)?;
    let mut args = Vec::new();
    if cfg!(target_os = "macos") {
        for section in ["__llvm_prf_cnts", "__llvm_prf_bits", "__llvm_prf_data"] {
            args.push(format!("-Wl,-sectalign,__DATA,{section},0x4000"));
        }
    } else {
        args.push("-Wl,-u,__llvm_profile_runtime".into());
    }
    args.push(rt.display().to_string());
    Ok(args)
}

/// The pinned toolchain behind the driver's LLVM targets. `opt`/`debug` pick the native runtime
/// flavor; wasm32 builds take the guest level from the compiler.
pub struct Toolchain {
    pub config: std::sync::Arc<crate::driver::toolchain::ToolchainConfig>,
    pub opt: OptLevel,
    pub debug: bool,
}

impl crate::driver::compiler::LlvmToolchain for Toolchain {
    fn runtime_sigs(
        &self,
        req: &crate::driver::compiler::LlvmRuntimeRequest,
    ) -> Result<crate::driver::compiler::RuntimeSignatures, String> {
        let config = &self.config;
        let tools = resolve_llvm(config)?;
        if !req.target.spec().capabilities.linear_memory && !req.target.spec().can_link_on_host() {
            return super::cross::runtime_signatures(&tools, req.target.spec());
        }
        let sigs = if req.target.spec().capabilities.linear_memory {
            super::wasm::wasm_runtime(&tools, req.wasm_opt, req.need, req.threads)?.sigs
        } else {
            llvm_runtime(&tools, req.target.spec(), self.opt, req.need, self.debug)?.sigs
        };
        let text =
            std::fs::read_to_string(&sigs).map_err(|e| format!("{}: {e}", sigs.display()))?;
        Ok(crate::driver::compiler::RuntimeSignatures {
            text,
            cache_path: sigs,
        })
    }

    fn link_wasm(
        &self,
        ll: &Path,
        wasm: &Path,
        opt_ll: Option<&Path>,
        req: &crate::driver::compiler::LlvmRuntimeRequest,
    ) -> Result<(), String> {
        let config = &self.config;
        let tools = resolve_llvm(config)?;
        super::wasm::link_wasm(
            &tools,
            ll,
            wasm,
            opt_ll,
            req.need,
            req.threads,
            req.wasm_opt,
        )
    }
}
