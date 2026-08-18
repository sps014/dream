//! Compile generated MIR C with the native runtime and run the resulting binary.

pub mod abi;
mod cc;

use crate::driver::wasm_opt::OptLevel;
use crate::execution::host::{cc_link_flags, read_c_libs_from_abi, search_roots_for_artifact};
use dream_mir::backend::c::{native_runtime_include_dir, native_runtime_units};
use dream_mir::runtime::runtime_need_from_c_source;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub fn compile_and_run(c_path: &str, opt: OptLevel) -> Result<(), Box<dyn std::error::Error>> {
    let bin = compile_native_c(Path::new(c_path), opt)?;
    run_native_bin(&bin, c_path)
}

pub fn compile_and_capture(
    c_path: &str,
    opt: OptLevel,
) -> Result<String, Box<dyn std::error::Error>> {
    let bin = compile_native_c(Path::new(c_path), opt)?;
    let mut cmd = Command::new(&bin);
    apply_native_run_env(&mut cmd, c_path);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let child = cmd.spawn()?;
    let pid = child.id();
    let waiter = std::thread::spawn(move || child.wait_with_output());
    let limit = Duration::from_secs(8);
    let start = Instant::now();
    while !waiter.is_finished() {
        if start.elapsed() > limit {
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let out = waiter.join().map_err(|_| "native C waiter panicked")??;
    if start.elapsed() > limit {
        return Err("native C program timed out".into());
    }
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        return Err(format!(
            "native C program failed (status {:?}): stderr={err} stdout={stdout}",
            out.status
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn run_native_bin(bin: &Path, c_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(bin);
    apply_native_run_env(&mut cmd, c_path);
    let status = cmd.status()?;
    if !status.success() {
        return Err(format!("native C program failed (status {status:?})").into());
    }
    Ok(())
}

fn apply_native_run_env(cmd: &mut Command, c_path: &str) {
    cmd.env("DREAM_NATIVE_C", c_path);
    if let Some(dir) = libdream_dir() {
        let key = if cfg!(target_os = "macos") {
            "DYLD_LIBRARY_PATH"
        } else {
            "LD_LIBRARY_PATH"
        };
        let mut paths = dir.display().to_string();
        if let Ok(prev) = std::env::var(key) {
            paths = format!("{paths}:{prev}");
        }
        cmd.env(key, paths);
    }
}

fn libdream_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "dream.dll"
    } else if cfg!(target_os = "macos") {
        "libdream.dylib"
    } else {
        "libdream.so"
    }
}

fn libdream_dir() -> Option<PathBuf> {
    let name = libdream_name();
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(p) = exe.parent() {
            dirs.push(p.to_path_buf());
            if p.file_name().and_then(|s| s.to_str()) == Some("deps") {
                if let Some(parent) = p.parent() {
                    dirs.push(parent.to_path_buf());
                }
            }
        }
    }
    if let Ok(td) = std::env::var("CARGO_TARGET_DIR") {
        dirs.push(PathBuf::from(&td).join("debug"));
        dirs.push(PathBuf::from(&td).join("release"));
    }
    dirs.push(PathBuf::from("target/debug"));
    dirs.push(PathBuf::from("target/release"));
    dirs.into_iter().find(|d| d.join(name).exists())
}

fn runtime_archive(
    opt: OptLevel,
    need: dream_mir::runtime::RuntimeNeed,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    static LOCK: Mutex<()> = Mutex::new(());
    let dir = cc::native_rt_cache_root()
        .join(opt.native_rt_subdir())
        .join(format!("need_{:x}", need.bits()));
    std::fs::create_dir_all(&dir)?;
    // Cross-process: each `dream` PID has its own Mutex, so parallel probe jobs can race `ar`.
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join(".lock"))?;
    lock_file.lock()?;
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let archive = dir.join("libdream_rt.a");
    let stamp = dir.join(".stamp");
    let units = native_runtime_units(need);
    let newest = units
        .iter()
        .filter_map(|u| std::fs::metadata(&u.path).ok()?.modified().ok())
        .max();
    let stale = match (
        newest,
        std::fs::metadata(&stamp)
            .ok()
            .and_then(|m| m.modified().ok()),
    ) {
        (Some(src), Some(st)) => src > st,
        _ => true,
    };
    if archive.exists() && !stale {
        return Ok(archive);
    }
    let mut cflags: Vec<&str> = vec!["-std=gnu11", "-pthread", "-w", "-c"];
    cflags.extend(opt.cc_flags());
    let toolchain = cc::resolve_cc()?;
    let mut objs = Vec::new();
    for (i, u) in units.iter().enumerate() {
        let obj = dir.join(format!("{i}.o"));
        let mut cmd = toolchain.cc_command();
        cmd.args(&cflags);
        for inc in &u.include_dirs {
            cmd.arg(format!("-I{}", inc.display()));
        }
        for d in &u.defines {
            cmd.arg(format!("-D{d}"));
        }
        let st = cmd.arg(&u.path).arg("-o").arg(&obj).status()?;
        if !st.success() {
            return Err(format!("cc -c failed for {}", u.path.display()).into());
        }
        objs.push(obj);
    }
    let _ = std::fs::remove_file(&archive);
    let mut ar = toolchain.ar_command();
    ar.arg("crs").arg(&archive);
    for o in &objs {
        ar.arg(o);
    }
    if !ar.status()?.success() {
        return Err("ar failed for native runtime".into());
    }
    std::fs::write(&stamp, b"ok")?;
    Ok(archive)
}

pub fn compile_native_c(
    c_path: &Path,
    opt: OptLevel,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let obj = c_path.with_extension("o");
    let bin = c_path.with_extension("bin");
    let src = std::fs::read_to_string(c_path)?;
    let need = runtime_need_from_c_source(&src);
    let toolchain = cc::resolve_cc()?;
    let rt = runtime_archive(opt, need)?;
    let warn = [
        "-std=gnu11",
        "-pthread",
        "-w",
        "-Wno-unused-function",
        "-Wno-unused-variable",
        "-Wno-unused-parameter",
    ];
    let include = format!("-I{}", native_runtime_include_dir().display());

    let mut ccmd = toolchain.cc_command();
    ccmd.args(opt.cc_flags());
    ccmd.args(warn);
    ccmd.arg(&include);
    ccmd.arg("-c").arg(c_path).arg("-o").arg(&obj);
    let status = ccmd.status()?;
    if !status.success() {
        return Err(format!("cc -c failed for {}", c_path.display()).into());
    }

    let mut lcmd = toolchain.cc_command();
    lcmd.args(opt.cc_flags());
    lcmd.args(warn);
    lcmd.arg(&obj);
    lcmd.arg(&rt);
    lcmd.args(["-lm", "-lpthread"]);
    if let Some(dir) = libdream_dir() {
        lcmd.arg(format!("-L{}", dir.display()));
        lcmd.arg("-ldream");
        lcmd.arg(format!("-Wl,-rpath,{}", dir.display()));
    }
    let abi_path = c_path.with_extension("abi.json");
    let c_libs = read_c_libs_from_abi(&abi_path);
    if !c_libs.is_empty() {
        let roots = search_roots_for_artifact(c_path);
        for flag in cc_link_flags(&c_libs, &roots) {
            lcmd.arg(flag);
        }
    }
    lcmd.arg("-o").arg(&bin);
    let status = lcmd.status()?;
    if !status.success() {
        return Err(format!("cc link failed for {}", obj.display()).into());
    }
    Ok(bin)
}
