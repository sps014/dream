//! `.ll` → wasm32: the guest runtime as bitcode (prebuilt in a release, else compiled by the dev
//! LLVM's clang against Dream's own freestanding headers), one whole-program module through the
//! pinned `llvm-link`/`opt`/`llc`, and LLVM's `wasm-ld`.
//!
//! Assembly units (`g0.s`: per-instance wasm globals) cannot be bitcode; they stay objects and
//! join at `wasm-ld`.

use super::build::{llc_level, pipeline, write_ir};
use super::bundle::{clang_rt, prebuilt_file, rt_dir, ClangRt, RtDir};
use super::runtime::{anchor_unit, disassemble, reduce_disassembly};
use super::tools::LlvmTools;
use crate::driver::rt_stamp;
use crate::driver::wasi::{guest_include_dirs, run_captured, unit_command, wasm_ld_command};
use crate::driver::wasm_opt::OptLevel;
use dream_mir::backend::llvm::RuntimeSigs;
use dream_mir::runtime::RuntimeNeed;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct WasmRuntime {
    pub bc: PathBuf,
    pub sigs: PathBuf,
    pub objs: Vec<PathBuf>,
}

/// Libcalls `llc` may emit after `opt` ran; internalizing them would let `opt` drop the runtime
/// libc's definitions and turn the calls into host imports.
const KEEP_PUBLIC: &[&str] = &["memcpy", "memmove", "memset", "memcmp"];

struct Unit {
    path: PathBuf,
    defines: Vec<String>,
    include_dirs: Vec<PathBuf>,
}

fn units(root: &Path, need: RuntimeNeed) -> Vec<Unit> {
    let mut out: Vec<Unit> = dream_mir::runtime::wasm32_runtime_c_files(root)
        .into_iter()
        .map(|path| Unit {
            path,
            defines: Vec::new(),
            include_dirs: Vec::new(),
        })
        .collect();
    out.extend(
        dream_mir::runtime::wasm32_linked_units(root, need)
            .into_iter()
            .map(|u| Unit {
                path: u.path,
                defines: u.defines,
                include_dirs: u.include_dirs,
            }),
    );
    out
}

fn is_asm(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()) == Some("s")
}

pub(super) fn flavor(threads: bool) -> &'static str {
    if threads {
        "wasm32-threads"
    } else {
        "wasm32"
    }
}

/// Assembly unit objects keep their unit index, so a prebuilt tree lists them by name.
fn asm_objs(dir: &Path, units: &[Unit]) -> Vec<PathBuf> {
    units
        .iter()
        .enumerate()
        .filter(|(_, u)| is_asm(&u.path))
        .map(|(i, _)| dir.join(format!("{i}.o")))
        .collect()
}

/// The WASI libc headers `scripts/fetch-dev-llvm.sh` unpacks beside the development LLVM.
pub(super) fn wasi_sysroot(clang: &Path) -> Result<PathBuf, String> {
    let root = clang
        .parent()
        .and_then(Path::parent)
        .unwrap_or(Path::new("."))
        .join("share/wasi-sysroot");
    if root.join("include/wasm32-wasip1").is_dir() {
        Ok(root)
    } else {
        Err(format!(
            "WASI headers not found at {}; run scripts/fetch-dev-llvm.sh",
            root.display()
        ))
    }
}

pub fn wasm_runtime(
    tools: &LlvmTools,
    opt: OptLevel,
    need: RuntimeNeed,
    threads: bool,
) -> Result<WasmRuntime, String> {
    match rt_dir(&tools.config, flavor(threads), opt, need) {
        RtDir::Prebuilt(dir) => {
            let mut objs: Vec<PathBuf> = std::fs::read_dir(&dir)
                .map_err(|e| format!("{}: {e}", dir.display()))?
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("o"))
                .collect();
            objs.sort();
            Ok(WasmRuntime {
                bc: prebuilt_file(&dir, "dream_rt.bc")?,
                sigs: prebuilt_file(&dir, "dream_rt.sigs")?,
                objs,
            })
        }
        RtDir::Cache(dir) => build_wasm_runtime(tools, opt, need, threads, &dir),
    }
}

/// Compiles the guest runtime for `opt`/`need` into `dir`, unless its stamp says it is current.
pub(super) fn build_wasm_runtime(
    tools: &LlvmTools,
    opt: OptLevel,
    need: RuntimeNeed,
    threads: bool,
    dir: &Path,
) -> Result<WasmRuntime, String> {
    static LOCK: Mutex<()> = Mutex::new(());
    let clang = tools.clang()?;
    let sysroot = wasi_sysroot(&clang)?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join(".lock"))
        .map_err(|e| e.to_string())?;
    lock_file.lock().map_err(|e| e.to_string())?;
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let units = units(&tools.config.runtime_c, need);
    let include_dirs = guest_include_dirs(&tools.config.runtime_c);
    let includes: Vec<&Path> = include_dirs.iter().map(PathBuf::as_path).collect();
    let bc = dir.join("dream_rt.bc");
    let sigs = dir.join("dream_rt.sigs");
    let objs = asm_objs(dir, &units);
    let stamp = dir.join(".stamp");
    let mut inputs: Vec<PathBuf> = units.iter().map(|u| u.path.clone()).collect();
    for d in include_dirs
        .iter()
        .chain(units.iter().flat_map(|u| &u.include_dirs))
    {
        if let Ok(rd) = std::fs::read_dir(d) {
            inputs.extend(rd.flatten().map(|e| e.path()).filter(|p| p.is_file()));
        }
    }
    inputs.push(clang.clone());
    inputs.push(tools.tool("llvm-link"));
    inputs.push(sysroot.join("include/wasm32-wasip1/string.h"));
    let fingerprint = rt_stamp::fingerprint(inputs);
    if bc.exists()
        && sigs.exists()
        && objs.iter().all(|o| o.exists())
        && rt_stamp::matches(&stamp, &fingerprint)
    {
        return Ok(WasmRuntime { bc, sigs, objs });
    }

    let mut parts = Vec::new();
    for (i, u) in units.iter().enumerate() {
        let asm = is_asm(&u.path);
        let out = dir.join(format!("{i}.{}", if asm { "o" } else { "bc" }));
        let mut cmd = unit_command(
            &clang,
            &sysroot,
            &u.path,
            &includes,
            &u.include_dirs,
            &u.defines,
            threads,
            opt,
            &format!("rt{i}.c"),
        );
        run_captured(
            cmd.arg("-o").arg(&out).arg(&u.path),
            &format!("clang ({})", u.path.display()),
        )?;
        if !asm {
            parts.push(out);
        }
    }
    let mut link = tools.command("llvm-link");
    link.args(&parts).arg("-o").arg(&bc);
    run_captured(&mut link, "llvm-link (wasm32 runtime)")?;

    let check = |src: &Path| {
        let mut cmd = unit_command(
            &clang,
            &sysroot,
            src,
            &includes,
            &[],
            &[],
            threads,
            opt,
            "anchor.c",
        );
        cmd.args([
            "-fsyntax-only",
            "-w",
            "-ferror-limit=0",
            "-fno-color-diagnostics",
        ])
        .arg(src)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
        .map_err(|e| e.to_string())
    };
    let compile = |src: &Path, out: &Path| {
        let mut cmd = unit_command(
            &clang,
            &sysroot,
            src,
            &includes,
            &[],
            &[],
            threads,
            opt,
            "anchor.c",
        );
        run_captured(cmd.arg("-w").arg("-o").arg(out).arg(src), "clang (anchor)")
    };
    let anchor = anchor_unit(dir, "dream_rt_wasm32.h", &check, &compile)?;
    let merged = dir.join("sigs.bc");
    let mut link = tools.command("llvm-link");
    link.arg(&bc).arg(&anchor).arg("-o").arg(&merged);
    run_captured(&mut link, "llvm-link (wasm32 runtime signatures)")?;
    std::fs::write(&sigs, reduce_disassembly(&disassemble(tools, &merged)?))
        .map_err(|e| e.to_string())?;
    for p in parts.iter().chain([&anchor, &merged]) {
        let _ = std::fs::remove_file(p);
    }
    std::fs::write(&stamp, fingerprint).map_err(|e| e.to_string())?;
    Ok(WasmRuntime { bc, sigs, objs })
}

/// Functions a generated `.ll` exports (`"wasm-export-name"` on its `define` line).
fn module_exports(ll: &str) -> impl Iterator<Item = &str> {
    ll.lines()
        .filter(|l| l.starts_with("define ") && l.contains("\"wasm-export-name\""))
        .filter_map(|l| {
            let at = l.find(" @")? + 2;
            let rest = &l[at..];
            Some(&rest[..rest.find('(')?])
        })
}

pub fn link_wasm(
    tools: &LlvmTools,
    ll_path: &Path,
    wasm_path: &Path,
    opt_ll: Option<&Path>,
    need: RuntimeNeed,
    threads: bool,
    opt: OptLevel,
) -> Result<(), String> {
    let rt = wasm_runtime(tools, opt, need, threads)?;
    let packages = super::wasm_sources::compile(tools, ll_path, threads, opt)?;
    let src = std::fs::read_to_string(ll_path).map_err(|e| e.to_string())?;
    let sigs_text = std::fs::read_to_string(&rt.sigs).map_err(|e| e.to_string())?;
    let sigs = RuntimeSigs::parse(&sigs_text)?;
    let mut public: Vec<String> = KEEP_PUBLIC.iter().map(|s| s.to_string()).collect();
    public.extend(sigs.exports.iter().cloned());
    public.extend(module_exports(&src).map(str::to_string));
    public.extend(packages.public);
    public.sort();
    public.dedup();

    let linked = ll_path.with_extension("linked.bc");
    let mut link = tools.command("llvm-link");
    link.arg(ll_path).arg(&rt.bc).args(&packages.bitcode);
    if let Some(shim) =
        super::c_shim::shim_bitcode(tools, &dream_abi::target::TargetSpec::wasm32(), ll_path)?
    {
        let mut modules = packages.bitcode.clone();
        modules.push(rt.bc.clone());
        super::wasm_sources::validate_signatures(tools, &modules, &shim)?;
        link.arg(shim);
    }
    link.arg("-o").arg(&linked);
    run_captured(&mut link, &format!("llvm-link ({})", ll_path.display()))?;
    let optimized = ll_path.with_extension("opt.bc");
    let mut o = tools.command("opt");
    o.arg(format!("-passes={}", pipeline(opt, false)))
        .arg(format!("-internalize-public-api-list={}", public.join(",")))
        .arg(&linked)
        .arg("-o")
        .arg(&optimized);
    let r = run_captured(&mut o, "opt");
    let _ = std::fs::remove_file(&linked);
    r?;
    if let Some(out) = opt_ll {
        if let Err(e) = write_ir(tools, &optimized, out) {
            let _ = std::fs::remove_file(&optimized);
            return Err(e);
        }
    }
    let obj = ll_path.with_extension("wasm.o");
    let mut llc = tools.command("llc");
    llc.arg(llc_level(opt, false))
        .arg("-filetype=obj")
        .arg(&optimized)
        .arg("-o")
        .arg(&obj);
    if packages.exceptions {
        llc.args([
            "-wasm-enable-eh",
            "-exception-model=wasm",
            "-wasm-use-legacy-eh=false",
        ]);
    }
    let r = run_captured(&mut llc, "llc");
    let _ = std::fs::remove_file(&optimized);
    r?;

    // compiler-rt supplies builtins such as `__multi3` (a 64-bit `__builtin_mul_overflow`);
    // `--allow-undefined` would otherwise turn a missing one into a host import that fails at
    // instantiation. As an archive after the objects, only referenced members are linked.
    let builtins = clang_rt(tools, ClangRt::WasmBuiltins { threads })?;
    let mut cmd = wasm_ld_command(&tools.optional_tool("wasm-ld")?, threads, opt);
    if !packages.libraries.is_empty() {
        cmd.arg("--fatal-warnings");
    }
    cmd.arg("-o")
        .arg(wasm_path)
        .arg(&obj)
        .args(&rt.objs)
        .args(&packages.libraries)
        .arg(builtins);
    let r = run_captured(&mut cmd, "wasm-ld");
    let _ = std::fs::remove_file(&obj);
    r?;
    if !packages.libraries.is_empty() {
        if let Err(e) = super::wasm_sources::validate_imports(ll_path, wasm_path, &sigs) {
            let _ = std::fs::remove_file(wasm_path);
            return Err(e);
        }
    }
    Ok(())
}
