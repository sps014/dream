//! Compile C-backend wasm32 output with wasi-sdk clang / wasm-ld.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::driver::wasm_opt::OptLevel;

pub fn wasi_clang() -> Option<PathBuf> {
    if let Ok(sdk) = std::env::var("WASI_SDK_PATH") {
        if !sdk.is_empty() {
            let clang = PathBuf::from(sdk).join("bin").join(clang_name());
            if clang.is_file() && clang.parent().is_some_and(is_wasi_bin_dir) {
                return Some(clang);
            }
        }
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let root = PathBuf::from(home).join(".dream").join("toolchains");
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&root) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir()
                && p.file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|n| n.starts_with("wasi-sdk-"))
            {
                dirs.push(p);
            }
        }
    }
    dirs.sort();
    dirs.into_iter()
        .rev()
        .map(|d| d.join("bin").join(clang_name()))
        .find(|p| p.is_file() && p.parent().is_some_and(is_wasi_bin_dir))
}

fn is_wasi_bin_dir(dir: &Path) -> bool {
    dir.join(if cfg!(windows) {
        "wasm-ld.exe"
    } else {
        "wasm-ld"
    })
    .is_file()
        && dir.join("clang.cfg").is_file()
}

fn clang_name() -> &'static str {
    if cfg!(windows) {
        "clang.exe"
    } else {
        "clang"
    }
}

pub fn compile_c_to_wasm32(
    c_path: &Path,
    wasm_path: &Path,
    threads: bool,
    opt: OptLevel,
) -> Result<(), String> {
    let clang = wasi_clang().ok_or_else(|| {
        "wasi-sdk clang not found; run `dreamer toolchain install wasi-sdk`".to_string()
    })?;
    let wasm_ld = clang.parent().unwrap().join(if cfg!(windows) {
        "wasm-ld.exe"
    } else {
        "wasm-ld"
    });
    if !wasm_ld.is_file() {
        return Err(format!("wasm-ld missing next to {}", clang.display()));
    }
    let inc_abi = dream_mir::runtime::runtime_abi_include_dir();
    let inc_wasm = dream_mir::runtime::wasm32_runtime_include_dir();
    let inc_native = dream_mir::runtime::native_runtime_include_dir();
    let inc_native_parent = inc_native.parent().unwrap_or(&inc_native);
    let includes = [inc_wasm.as_path(), inc_abi.as_path(), inc_native_parent];
    let obj_dir = wasm_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = wasm_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("dream");
    let mut objs: Vec<PathBuf> = Vec::new();
    let guest_o = obj_dir.join(format!("{stem}_guest.o"));
    compile_unit(
        &clang,
        c_path,
        &guest_o,
        &includes,
        threads,
        opt,
        "guest.c",
    )?;
    objs.push(guest_o);
    for (i, src) in dream_mir::runtime::wasm32_runtime_c_files()
        .into_iter()
        .enumerate()
    {
        let obj = obj_dir.join(format!("{stem}_rt_{i}.o"));
        compile_unit(
            &clang,
            &src,
            &obj,
            &includes,
            threads,
            opt,
            &format!("rt{i}.c"),
        )?;
        objs.push(obj);
    }
    let mut cmd = Command::new(&wasm_ld);
    cmd.args([
        "--no-entry",
        "--allow-undefined",
        "--import-memory",
        "--export-memory",
        "--export-table",
        "--export=__stack_pointer",
        "--export=__tls_base",
        "--export=dream_malloc",
        "--gc-sections",
        "--strip-debug",
    ]);
    if threads {
        let max_bytes = u64::from(dream_mir::abi::MAX_MEMORY_PAGES)
            * u64::from(dream_mir::abi::WASM_PAGE_SIZE);
        cmd.arg("--shared-memory");
        cmd.arg(format!("--max-memory={max_bytes}"));
        // Clang's wasm32 default feature set is wider than atomics/bulk-memory. Restricting
        // `--features` to those two makes wasm-ld reject the rest (`sign-ext`, …).
        cmd.arg("--no-check-features");
    }
    cmd.arg("-o").arg(wasm_path);
    for o in &objs {
        cmd.arg(o);
    }
    let status = cmd.status().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("wasm-ld failed for wasm32 C guest".into());
    }
    Ok(())
}

fn compile_unit(
    clang: &Path,
    src: &Path,
    obj: &Path,
    includes: &[&Path],
    threads: bool,
    opt: OptLevel,
    stable_name: &str,
) -> Result<(), String> {
    let is_asm = src.extension().and_then(|e| e.to_str()) == Some("s");
    let mut cmd = Command::new(clang);
    cmd.args(["--target=wasm32-wasip1", "-nostdlib", "-c", "-g0"]);
    if is_asm {
        cmd.arg("-Wno-unused-command-line-argument");
    } else {
        cmd.args([
            opt.wasm_clang_opt_flag(),
            "-fno-ident",
            "-fno-exceptions",
            "-fno-builtin",
            "-ffunction-sections",
            "-fdata-sections",
            "-frandom-seed=0",
            "-Wno-unused-value",
            "-DDREAM_WASM32",
        ]);
        cmd.arg(format!(
            "-ffile-prefix-map={}={}",
            src.display(),
            stable_name
        ));
        if threads {
            cmd.args(["-matomics", "-mbulk-memory", "-DDREAM_WASM32_THREADS"]);
        }
        for inc in includes {
            cmd.arg("-I").arg(inc);
        }
    }
    let status = cmd
        .arg("-o")
        .arg(obj)
        .arg(src)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!(
            "clang failed for {} (using {})",
            src.display(),
            clang.display()
        ));
    }
    Ok(())
}
