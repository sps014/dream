//! `.ll` → wasm32: the guest runtime as bitcode from wasi-sdk clang, one whole-program module
//! through the pinned `llvm-link`/`opt`/`llc`, and wasi-sdk `wasm-ld`.
//!
//! Assembly units (`g0.s`: per-instance wasm globals) cannot be bitcode; they stay objects and
//! join at `wasm-ld`.

use super::build::{llc_level, pipeline};
use super::runtime::{anchor_unit, reduce_disassembly};
use super::tools::LlvmTools;
use crate::driver::rt_stamp;
use crate::driver::wasi::{
    compiler_rt_builtins, guest_include_dirs, run_captured, unit_command, wasi_clang,
    wasm_ld_command, wasm_ld_for,
};
use crate::driver::wasm_opt::OptLevel;
use crate::execution::native::cc::native_rt_cache_root;
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

fn units(need: RuntimeNeed) -> Vec<Unit> {
    let native_inc = dream_mir::runtime::native_runtime_include_dir();
    let mut out: Vec<Unit> = dream_mir::runtime::wasm32_runtime_c_files()
        .into_iter()
        .map(|path| Unit {
            path,
            defines: Vec::new(),
            include_dirs: Vec::new(),
        })
        .collect();
    if let Some(native) = native_inc.parent() {
        out.push(Unit {
            path: native.join("llvm_inline.c"),
            defines: Vec::new(),
            include_dirs: vec![native_inc.clone()],
        });
    }
    out.extend(
        dream_mir::runtime::wasm32_linked_units(need)
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

pub fn wasm_runtime(
    tools: &LlvmTools,
    opt: OptLevel,
    need: RuntimeNeed,
    threads: bool,
) -> Result<WasmRuntime, String> {
    static LOCK: Mutex<()> = Mutex::new(());
    let clang = wasi_clang().ok_or_else(|| {
        "wasi-sdk clang not found; run `dreamer toolchain install wasi-sdk`".to_string()
    })?;
    let dir = native_rt_cache_root()
        .join(format!("llvm-{}", super::LLVM_VERSION))
        .join(if threads { "wasm32-threads" } else { "wasm32" })
        .join(opt.native_rt_subdir())
        .join(format!("need_{:x}", need.bits()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join(".lock"))
        .map_err(|e| e.to_string())?;
    lock_file.lock().map_err(|e| e.to_string())?;
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let units = units(need);
    let include_dirs = guest_include_dirs();
    let includes: Vec<&Path> = include_dirs.iter().map(PathBuf::as_path).collect();
    let bc = dir.join("dream_rt.bc");
    let sigs = dir.join("dream_rt.sigs");
    let objs: Vec<PathBuf> = units
        .iter()
        .enumerate()
        .filter(|(_, u)| is_asm(&u.path))
        .map(|(i, _)| dir.join(format!("{i}.o")))
        .collect();
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
        let mut cmd = unit_command(&clang, src, &includes, &[], &[], threads, opt, "anchor.c");
        cmd.args(["-fsyntax-only", "-w", "-ferror-limit=0"])
            .arg(src)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
            .map_err(|e| e.to_string())
    };
    let compile = |src: &Path, out: &Path| {
        let mut cmd = unit_command(&clang, src, &includes, &[], &[], threads, opt, "anchor.c");
        run_captured(cmd.arg("-w").arg("-o").arg(out).arg(src), "clang (anchor)")
    };
    let anchor = anchor_unit(&dir, "dream_rt_wasm32.h", &check, &compile)?;
    let merged = dir.join("sigs.bc");
    let mut link = tools.command("llvm-link");
    link.arg(&bc).arg(&anchor).arg("-o").arg(&merged);
    run_captured(&mut link, "llvm-link (wasm32 runtime signatures)")?;
    let out = tools
        .command("llvm-dis")
        .arg(&merged)
        .arg("-o")
        .arg("-")
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "llvm-dis failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    std::fs::write(
        &sigs,
        reduce_disassembly(&String::from_utf8_lossy(&out.stdout)),
    )
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
    need: RuntimeNeed,
    threads: bool,
    opt: OptLevel,
) -> Result<(), String> {
    let rt = wasm_runtime(tools, opt, need, threads)?;
    let src = std::fs::read_to_string(ll_path).map_err(|e| e.to_string())?;
    let sigs_text = std::fs::read_to_string(&rt.sigs).map_err(|e| e.to_string())?;
    let sigs = RuntimeSigs::parse(&sigs_text)?;
    let mut public: Vec<String> = KEEP_PUBLIC.iter().map(|s| s.to_string()).collect();
    public.extend(sigs.exports.iter().cloned());
    public.extend(module_exports(&src).map(str::to_string));
    public.sort();
    public.dedup();

    let linked = ll_path.with_extension("linked.bc");
    let mut link = tools.command("llvm-link");
    link.arg(ll_path).arg(&rt.bc).arg("-o").arg(&linked);
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
    let obj = ll_path.with_extension("wasm.o");
    let mut llc = tools.command("llc");
    llc.arg(llc_level(opt, false))
        .arg("-filetype=obj")
        .arg(&optimized)
        .arg("-o")
        .arg(&obj);
    let r = run_captured(&mut llc, "llc");
    let _ = std::fs::remove_file(&optimized);
    r?;

    let clang = wasi_clang().ok_or_else(|| {
        "wasi-sdk clang not found; run `dreamer toolchain install wasi-sdk`".to_string()
    })?;
    let mut cmd = wasm_ld_command(&wasm_ld_for(&clang)?, threads, opt);
    cmd.arg("-o").arg(wasm_path).arg(&obj).args(&rt.objs);
    cmd.arg(compiler_rt_builtins(&clang, threads)?);
    let r = run_captured(&mut cmd, "wasm-ld");
    let _ = std::fs::remove_file(&obj);
    r
}
