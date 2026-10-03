//! `.ll` → native binary: `llvm-link` with the runtime bitcode, `opt` (internalize to `main`, then
//! the standard pipeline), `llc` to an object, and the system C compiler as the linker only.

use super::c_shim::shim_bitcode;
use super::icon;
use super::runtime::llvm_runtime;
use super::tools::{resolve_llvm, LlvmTools};
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
use dream_mir::runtime::runtime_need_from_module_text;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

/// The new-pass-manager pipeline for a level. Debug builds stay at `O0` so values survive.
pub(super) fn pipeline(opt: OptLevel, debug: bool) -> &'static str {
    if debug {
        return "internalize,default<O0>";
    }
    match opt {
        OptLevel::O0 => "internalize,default<O0>",
        OptLevel::O1 => "internalize,default<O1>",
        OptLevel::O2 => "internalize,default<O2>",
        OptLevel::O3 | OptLevel::O4 => "internalize,default<O3>",
        OptLevel::Size => "internalize,default<Os>",
        OptLevel::SizeAggressive => "internalize,default<Oz>",
    }
}

pub(super) fn llc_level(opt: OptLevel, debug: bool) -> &'static str {
    if debug {
        return "-O0";
    }
    match opt {
        OptLevel::O0 => "-O0",
        OptLevel::O1 => "-O1",
        OptLevel::O2 | OptLevel::Size | OptLevel::SizeAggressive => "-O2",
        OptLevel::O3 | OptLevel::O4 => "-O3",
    }
}

/// The CPU for the program and its runtime (whose bitcode carries none): the host's own at
/// `-O3`/`-O4`, where the binary is built to run here; otherwise the target's baseline, which on
/// Apple silicon is the M1 every arm64 Mac has.
pub(super) fn cpu_args(opt: OptLevel, debug: bool) -> &'static [&'static str] {
    if !debug && matches!(opt, OptLevel::O3 | OptLevel::O4) {
        &["-mcpu=native"]
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        &["-mcpu=apple-m1"]
    } else {
        &[]
    }
}

/// `opt`'s PGO pipeline kind and its profile file (see `pgo::llvm_pgo`).
type PgoPipeline = Option<(&'static str, PathBuf)>;

/// Runs `opt` over the linked module and returns the optimized bitcode path.
fn optimize_linked(
    tools: &LlvmTools,
    linked: &Path,
    opt: OptLevel,
    debug: bool,
    pgo: &PgoPipeline,
    exports: &[String],
) -> Result<PathBuf, String> {
    let out = linked.with_extension("opt.bc");
    let mut cmd = tools.command("opt");
    cmd.arg(format!("-passes={}", pipeline(opt, debug)))
        .args(cpu_args(opt, debug))
        .arg(public_api_list(exports));
    if let Some((kind, file)) = pgo {
        cmd.arg(format!("-pgo-kind={kind}"))
            .arg(format!("-profile-file={}", file.display()));
    }
    cmd.arg(linked).arg("-o").arg(&out);
    run_captured(&mut cmd, "opt")?;
    Ok(out)
}

/// `main`, the embedding API, and the runtime functions compiled native sources call into.
fn public_api_list(exports: &[String]) -> String {
    let mut list = vec!["main"];
    list.extend(EMBED_EXPORTS);
    list.extend(
        exports
            .iter()
            .map(String::as_str)
            .filter(|e| !EMBED_EXPORTS.contains(e)),
    );
    format!("-internalize-public-api-list={}", list.join(","))
}

pub fn native_bin_path(ll_path: &Path) -> PathBuf {
    ll_path.with_extension("bin")
}

/// Links `ll_path` with the extra bitcode/IR `modules` (runtime first), then optimizes.
fn link_and_optimize(
    tools: &LlvmTools,
    ll_path: &Path,
    modules: &[&Path],
    opt: OptLevel,
    debug: bool,
    pgo: &PgoPipeline,
    exports: &[String],
) -> Result<PathBuf, String> {
    let linked = ll_path.with_extension("linked.bc");
    let mut link = tools.command("llvm-link");
    link.arg(ll_path).args(modules).arg("-o").arg(&linked);
    run_captured(&mut link, &format!("llvm-link ({})", ll_path.display()))?;
    let optimized = optimize_linked(tools, &linked, opt, debug, pgo, exports);
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
) -> Result<(), String> {
    let mut llc = tools.command("llc");
    llc.arg(llc_level(opt, debug))
        .args(section_args())
        .args(cpu_args(opt, debug))
        .arg(format!("-filetype={filetype}"))
        .arg("-relocation-model=pic")
        .arg(input)
        .arg("-o")
        .arg(out);
    run_captured(&mut llc, "llc")
}

fn section_args() -> &'static [&'static str] {
    if cfg!(target_os = "linux") {
        &["-function-sections", "-data-sections"]
    } else {
        &[]
    }
}

fn dead_strip_args() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["-Wl,-dead_strip"]
    } else if cfg!(target_os = "linux") {
        &["-Wl,--gc-sections"]
    } else {
        &[]
    }
}

/// Disassembles bitcode `bc` to textual IR at `out`.
pub(super) fn write_ir(tools: &LlvmTools, bc: &Path, out: &Path) -> Result<(), String> {
    let mut dis = tools.command("llvm-dis");
    dis.arg(bc).arg("-o").arg(out);
    run_captured(&mut dis, "llvm-dis")
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
        opt,
        debug,
        &None,
        &[],
    );
    for p in icon_ll.iter().chain(&shim) {
        let _ = std::fs::remove_file(p);
    }
    let optimized = optimized?;
    let opt_ll = ll_path.with_extension("opt.ll");
    write_ir(&tools, &optimized, &opt_ll)?;
    let asm = ll_path.with_extension("s");
    run_llc(&tools, &optimized, opt, debug, "asm", &asm)?;
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
    } = options;
    if !spec.can_link_on_host() {
        return Err(format!(
            "native linking is host-only; use --target {} to emit .ll and .o",
            spec.triple
        )
        .into());
    }
    let tools = resolve_llvm(config)?;
    let bin = native_bin_path(ll_path);
    let abi_path = ll_path.with_extension("abi.json");
    let capabilities = read_host_capabilities(ll_path)?;
    let dir = host_library_dir(config, &capabilities).ok_or(
        "native capability libraries not found next to the dream binary. \
         Build with cargo build --workspace, or set DREAM_HOME or DREAM_BIN to the installed toolchain.",
    )?;
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
        compile_sets(config, &cc::resolve_cc(config)?, &c_sources, &cache, debug)?
    };
    let stamp_path = bin.with_extension("flags");
    let stamp = format!(
        "native-c-shim-v3\n{}\n{}\n{}\n{}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{}\n{:?}\n{:?}\n{:?}",
        pipeline(opt, debug),
        debug,
        llc_level(opt, debug),
        tools.bin.display(),
        rt.archive,
        profile,
        icon_png.as_deref().map(icon::fingerprint),
        native.objects,
        native.link_args,
        relocatable,
        capabilities,
        section_args(),
        dead_strip_args()
    );
    let input = match (pgo, &profile) {
        (Pgo::Use(_), Some((_, p))) => Some(p.as_path()),
        _ => None,
    };
    if native_bin_fresh(&bin, ll_path, &rt.bc, input)
        && native.objects.iter().all(|o| !newer_than(o, &bin))
        && opt_ll.is_none_or(Path::exists)
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
    let shim = shim_bitcode(&tools, &spec, ll_path)?;
    let optimized = link_and_optimize(
        &tools,
        ll_path,
        &[Some(rt.bc.as_path()), shim.as_deref(), icon_ll.as_deref()]
            .iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>(),
        opt,
        debug,
        &profile,
        &native.runtime_exports,
    );
    for p in icon_ll.iter().chain(&shim) {
        let _ = std::fs::remove_file(p);
    }
    let optimized = optimized?;
    if let Some(out) = opt_ll {
        write_ir(&tools, &optimized, out)?;
    }
    let obj = ll_path.with_extension("o");
    run_llc(&tools, &optimized, opt, debug, "obj", &obj)?;

    let mut lcmd = if *pgo == Pgo::Generate {
        let mut c = std::process::Command::new(cc::resolve_system_cc(config).ok_or(PGO_NEEDS_CC)?);
        c.arg(&obj).args(profile_link_args(&tools)?);
        c
    } else {
        let cc = cc::resolve_cc(config)?;
        let mut c = if native.needs_cxx && cfg!(all(windows, target_env = "msvc")) {
            cc.cxx_command(config)?
        } else {
            cc.cc_command()
        };
        c.arg(&obj);
        c
    };
    lcmd.args(&native.objects);
    if let Some(version) = spec.min_os {
        lcmd.arg(format!(
            "-mmacosx-version-min={}.{}.{}",
            version.major, version.minor, version.patch
        ));
    }
    lcmd.args(dead_strip_args());
    if let Some(a) = &rt.archive {
        lcmd.arg(a);
    }
    #[cfg(windows)]
    if let Some(png) = &icon_png {
        lcmd.arg(icon::windows_resource(&tools, ll_path, png)?);
    }
    if !cfg!(windows) {
        lcmd.args(["-lm", "-lpthread"]);
    }
    link_runtime(&mut lcmd, &dir, bundled.as_deref(), &capabilities);
    let c_libs = read_c_libs_from_abi(&abi_path);
    if !c_libs.is_empty() {
        let roots = search_roots_for_artifact(config, ll_path);
        lcmd.args(cc_link_flags(config, &c_libs, &roots));
    }
    lcmd.args(&native.link_args);
    lcmd.arg("-o").arg(&bin);
    if let Err(e) = run_captured(&mut lcmd, &format!("cc link ({})", obj.display())) {
        let _ = std::fs::remove_file(&bin);
        return Err(e.into());
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

#[cfg(test)]
mod size_tests {
    use super::*;

    #[test]
    fn native_dead_stripping_matches_object_section_policy() {
        if cfg!(target_os = "linux") {
            assert_eq!(section_args(), &["-function-sections", "-data-sections"]);
            assert_eq!(dead_strip_args(), &["-Wl,--gc-sections"]);
        } else if cfg!(target_os = "macos") {
            assert!(section_args().is_empty());
            assert_eq!(dead_strip_args(), &["-Wl,-dead_strip"]);
        } else {
            assert!(section_args().is_empty());
            assert!(dead_strip_args().is_empty());
        }
    }
}
