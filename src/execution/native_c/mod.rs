//! Compile generated MIR C with the native runtime and run the resulting binary.

pub mod abi;
pub mod webview;
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
    let bin = compile_native_c(Path::new(c_path), opt, false)?;
    run_native_bin(&bin, c_path, &[])
}

pub fn compile_and_capture(
    c_path: &str,
    opt: OptLevel,
) -> Result<String, Box<dyn std::error::Error>> {
    compile_and_capture_with_env(c_path, opt, &[])
}

pub fn compile_and_capture_with_env(
    c_path: &str,
    opt: OptLevel,
    extra_env: &[(&str, &str)],
) -> Result<String, Box<dyn std::error::Error>> {
    compile_and_capture_ex(c_path, opt, extra_env, &[], None, 8)
}

pub fn compile_and_capture_ex(
    c_path: &str,
    opt: OptLevel,
    extra_env: &[(&str, &str)],
    extra_args: &[&str],
    stdin: Option<&[u8]>,
    timeout_secs: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    let bin = compile_native_c(Path::new(c_path), opt, false)?;
    let mut cmd = Command::new(&bin);
    apply_native_run_env(&mut cmd, c_path);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    cmd.args(extra_args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }
    let mut child = cmd.spawn()?;
    if let Some(bytes) = stdin {
        if let Some(mut sin) = child.stdin.take() {
            let _ = std::io::Write::write_all(&mut sin, bytes);
        }
    }
    let pid = child.id();
    let waiter = std::thread::spawn(move || child.wait_with_output());
    let limit = Duration::from_secs(timeout_secs);
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

pub fn run_native_bin(
    bin: &Path,
    c_path: &str,
    extra_args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(bin);
    apply_native_run_env(&mut cmd, c_path);
    cmd.args(extra_args);
    let status = cmd.status()?;
    if !status.success() {
        return Err(format!("native C program failed (status {status:?})").into());
    }
    Ok(())
}

pub(crate) fn apply_native_run_env(cmd: &mut Command, c_path: &str) {
    for (k, v) in native_run_env_pairs(c_path) {
        cmd.env(k, v);
    }
}

/// Env vars the native guest needs (`DREAM_NATIVE_C`, dylib search path). Used by `dream run`
/// and the lldb-dap debug adapter.
pub(crate) fn native_run_env_pairs(c_path: &str) -> Vec<(String, String)> {
    let mut out = vec![("DREAM_NATIVE_C".to_string(), c_path.to_string())];
    if let Some(dir) = libdream_dir() {
        let key = if cfg!(target_os = "macos") {
            "DYLD_LIBRARY_PATH"
        } else if cfg!(target_os = "windows") {
            "PATH"
        } else {
            "LD_LIBRARY_PATH"
        };
        let mut paths = dir.display().to_string();
        if let Ok(prev) = std::env::var(key) {
            paths = format!("{paths}:{prev}");
        }
        out.push((key.to_string(), paths));
    }
    out
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

fn push_libdream_dir(dirs: &mut Vec<PathBuf>, dir: PathBuf) {
    if !dir.as_os_str().is_empty() && !dirs.iter().any(|d| d == &dir) {
        dirs.push(dir);
    }
}

fn push_exe_parent(dirs: &mut Vec<PathBuf>, exe: &Path) {
    if let Some(p) = exe.parent() {
        push_libdream_dir(dirs, p.to_path_buf());
        if p.file_name().and_then(|s| s.to_str()) == Some("deps") {
            if let Some(parent) = p.parent() {
                push_libdream_dir(dirs, parent.to_path_buf());
            }
        }
    }
}

fn libdream_dir() -> Option<PathBuf> {
    let name = libdream_name();
    let mut dirs = Vec::new();
    // `~/.dream/bin/dream` is a symlink; `current_exe` is often the link path, which does
    // not contain libdream. Prefer DREAM_HOME / the canonical binary dir.
    if let Ok(home) = std::env::var("DREAM_HOME") {
        if !home.is_empty() {
            push_libdream_dir(&mut dirs, PathBuf::from(home));
        }
    }
    if let Ok(bin) = std::env::var("DREAM_BIN") {
        if let Some(p) = Path::new(&bin).parent() {
            push_libdream_dir(&mut dirs, p.to_path_buf());
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(canon) = exe.canonicalize() {
            push_exe_parent(&mut dirs, &canon);
        }
        push_exe_parent(&mut dirs, &exe);
    }
    if let Ok(td) = std::env::var("CARGO_TARGET_DIR") {
        push_libdream_dir(&mut dirs, PathBuf::from(&td).join("debug"));
        push_libdream_dir(&mut dirs, PathBuf::from(&td).join("release"));
    }
    push_libdream_dir(&mut dirs, PathBuf::from("target/debug"));
    push_libdream_dir(&mut dirs, PathBuf::from("target/release"));
    dirs.into_iter().find(|d| d.join(name).exists())
}

const DEBUG_CC_FLAGS: &[&str] = &["-g", "-O0", "-fno-omit-frame-pointer"];

fn runtime_archive(
    opt: OptLevel,
    need: dream_mir::runtime::RuntimeNeed,
    debug: bool,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    static LOCK: Mutex<()> = Mutex::new(());
    let sub = if debug {
        "O0-g"
    } else {
        opt.native_rt_subdir()
    };
    let dir = cc::native_rt_cache_root()
        .join(sub)
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
    if debug {
        cflags.extend(DEBUG_CC_FLAGS);
    } else {
        cflags.extend(opt.cc_flags());
    }
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
    for o in &objs {
        let _ = std::fs::remove_file(o);
    }
    std::fs::write(&stamp, b"ok")?;
    Ok(archive)
}

pub fn compile_native_c(
    c_path: &Path,
    opt: OptLevel,
    debug: bool,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let obj = c_path.with_extension("o");
    let bin = c_path.with_extension("bin");
    // Parallel `dream` processes (probe / json harness) share this path.
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(bin.with_extension("cc.lock"))?;
    lock_file.lock()?;
    let src = std::fs::read_to_string(c_path)?;
    let need = runtime_need_from_c_source(&src);
    let toolchain = cc::resolve_cc()?;
    let rt = runtime_archive(opt, need, debug)?;
    if native_bin_fresh(&bin, c_path, &rt) {
        return Ok(bin);
    }
    let warn = [
        "-std=gnu11",
        "-pthread",
        "-w",
        "-Wno-unused-function",
        "-Wno-unused-variable",
        "-Wno-unused-parameter",
    ];
    let include = format!("-I{}", native_runtime_include_dir().display());
    let opt_flags: &[&str] = if debug {
        DEBUG_CC_FLAGS
    } else {
        opt.cc_flags()
    };

    let mut ccmd = toolchain.cc_command();
    ccmd.args(opt_flags);
    ccmd.args(warn);
    ccmd.arg(&include);
    if crate::driver::ui::color_enabled() {
        // We capture output, so force colored diagnostics (clang + gcc spelling).
        ccmd.arg("-fdiagnostics-color=always");
    }
    ccmd.arg("-c").arg(c_path).arg("-o").arg(&obj);
    crate::driver::c_wasm32::run_captured(&mut ccmd, &format!("cc -c ({})", c_path.display()))?;

    let mut lcmd = toolchain.cc_command();
    lcmd.args(opt_flags);
    lcmd.args(warn);
    lcmd.arg(&obj);
    lcmd.arg(&rt);
    lcmd.args(["-lm", "-lpthread"]);
    let Some(dir) = libdream_dir() else {
        return Err(
            "libdream not found next to the dream binary (needed to link host functions). \
             Set DREAM_HOME or DREAM_BIN to the directory containing libdream."
                .into(),
        );
    };
    lcmd.arg(format!("-L{}", dir.display()));
    lcmd.arg("-ldream");
    lcmd.arg(format!("-Wl,-rpath,{}", dir.display()));
    let abi_path = c_path.with_extension("abi.json");
    let c_libs = read_c_libs_from_abi(&abi_path);
    if !c_libs.is_empty() {
        let roots = search_roots_for_artifact(c_path);
        for flag in cc_link_flags(&c_libs, &roots) {
            lcmd.arg(flag);
        }
    }
    lcmd.arg("-o").arg(&bin);
    crate::driver::c_wasm32::run_captured(&mut lcmd, &format!("cc link ({})", obj.display()))?;
    let _ = std::fs::remove_file(&obj);
    Ok(bin)
}

fn mtime(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn native_bin_fresh(bin: &Path, c_path: &Path, rt: &Path) -> bool {
    let Some(bin_t) = mtime(bin) else {
        return false;
    };
    mtime(c_path).is_some_and(|t| t <= bin_t) && mtime(rt).is_some_and(|t| t <= bin_t)
}
