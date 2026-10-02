//! `.ll` → native binary: `llvm-link` with the runtime bitcode, `opt` (internalize to `main`, then
//! the standard pipeline), `llc` to an object, and the system C compiler as the linker only.

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
use crate::execution::native::{cc, libdream_dir, native_bin_fresh, Pgo};
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

/// `main` plus the runtime functions compiled native sources call into.
fn public_api_list(exports: &[String]) -> String {
    let mut list = vec!["main"];
    list.extend(exports.iter().map(String::as_str));
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
        .args(cpu_args(opt, debug))
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
    run_captured(&mut dis, "llvm-dis")
}

/// `--emit-llvm`: the optimized whole-program module as `<stem>.opt.ll` and its assembly as
/// `<stem>.s`, next to the `.ll`.
pub fn emit_llvm_artifacts(
    ll_path: &Path,
    opt: OptLevel,
    debug: bool,
    icon: Option<&Path>,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let tools = resolve_llvm()?;
    let src = std::fs::read_to_string(ll_path)?;
    let rt = llvm_runtime(&tools, opt, runtime_need_from_module_text(&src), debug)?;
    let icon_ll = match icon {
        Some(icon) => Some(icon::write_icon_module(ll_path, &icon::read_png(icon)?, &src)?),
        None => None,
    };
    let optimized = link_and_optimize(
        &tools,
        ll_path,
        &[Some(rt.bc.as_path()), icon_ll.as_deref()]
            .iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>(),
        opt,
        debug,
        &None,
        &[],
    );
    if let Some(p) = &icon_ll {
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
pub fn compile_llvm(
    ll_path: &Path,
    opt_ll: Option<&Path>,
    opt: OptLevel,
    debug: bool,
    pgo: &Pgo,
    icon: Option<&Path>,
    relocatable: bool,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let tools = resolve_llvm()?;
    let bin = native_bin_path(ll_path);
    let dir = libdream_dir().ok_or(
        "libdream not found next to the dream binary (needed to link host functions). \
         Build it with cargo build -p dream-host, or set DREAM_HOME or DREAM_BIN to the directory containing libdream.",
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
        )?)
    } else {
        None
    };
    let src = std::fs::read_to_string(ll_path)?;
    let need = runtime_need_from_module_text(&src);
    let rt = llvm_runtime(&tools, opt, need, debug)?;
    let profile = llvm_pgo(pgo, &bin, || tools.optional_tool("llvm-profdata"))?;
    let icon_png = icon.map(icon::read_png).transpose()?;
    let abi_path = ll_path.with_extension("abi.json");
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
        compile_sets(&cc::resolve_cc()?, &c_sources, &cache, debug)?
    };
    let stamp_path = bin.with_extension("flags");
    let stamp = format!(
        "{}\n{}\n{}\n{}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{}",
        pipeline(opt, debug),
        debug,
        llc_level(opt, debug),
        tools.bin.display(),
        rt.archive,
        profile,
        icon_png.as_deref().map(icon::fingerprint),
        native.objects,
        native.link_args,
        relocatable
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
    let optimized = link_and_optimize(
        &tools,
        ll_path,
        &[Some(rt.bc.as_path()), icon_ll.as_deref()]
            .iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>(),
        opt,
        debug,
        &profile,
        &native.runtime_exports,
    );
    if let Some(p) = &icon_ll {
        let _ = std::fs::remove_file(p);
    }
    let optimized = optimized?;
    if let Some(out) = opt_ll {
        write_ir(&tools, &optimized, out)?;
    }
    let obj = ll_path.with_extension("o");
    run_llc(&tools, &optimized, opt, debug, "obj", &obj)?;

    let mut lcmd = if *pgo == Pgo::Generate {
        let mut c = std::process::Command::new(cc::resolve_system_cc().ok_or(PGO_NEEDS_CC)?);
        c.arg(&obj).args(profile_link_args(&tools)?);
        c
    } else {
        let mut c = cc::resolve_cc()?.cc_command();
        c.arg(&obj);
        c
    };
    lcmd.args(&native.objects);
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
    link_runtime(&mut lcmd, &dir, bundled.as_deref());
    let c_libs = read_c_libs_from_abi(&abi_path);
    if !c_libs.is_empty() {
        let roots = search_roots_for_artifact(ll_path);
        lcmd.args(cc_link_flags(&c_libs, &roots));
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
    pub opt: OptLevel,
    pub debug: bool,
}

impl crate::driver::compiler::LlvmToolchain for Toolchain {
    fn runtime_sigs(
        &self,
        req: &crate::driver::compiler::LlvmRuntimeRequest,
    ) -> Result<String, String> {
        let tools = resolve_llvm()?;
        let sigs = if req.target.is_wasm32() {
            super::wasm::wasm_runtime(&tools, req.wasm_opt, req.need, req.threads)?.sigs
        } else {
            llvm_runtime(&tools, self.opt, req.need, self.debug)?.sigs
        };
        std::fs::read_to_string(&sigs).map_err(|e| format!("{}: {e}", sigs.display()))
    }

    fn link_wasm(
        &self,
        ll: &Path,
        wasm: &Path,
        opt_ll: Option<&Path>,
        req: &crate::driver::compiler::LlvmRuntimeRequest,
    ) -> Result<(), String> {
        let tools = resolve_llvm()?;
        super::wasm::link_wasm(&tools, ll, wasm, opt_ll, req.need, req.threads, req.wasm_opt)
    }
}
