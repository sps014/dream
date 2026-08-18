//! Compile generated MIR C with the native runtime and run the resulting binary.

pub mod abi;

use dream_mir::backend::c::{native_runtime_c_files, native_runtime_include_dir};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub fn compile_and_run(c_path: &str, release: bool) -> Result<(), Box<dyn std::error::Error>> {
    let bin = compile_native_c(Path::new(c_path), release)?;
    let mut cmd = Command::new(&bin);
    apply_native_run_env(&mut cmd, c_path);
    let status = cmd.status()?;
    if !status.success() {
        return Err(format!("native C program failed (status {status:?})").into());
    }
    Ok(())
}

pub fn compile_and_capture(
    c_path: &str,
    release: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let bin = compile_native_c(Path::new(c_path), release)?;
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

fn runtime_archive() -> Result<PathBuf, Box<dyn std::error::Error>> {
    static LOCK: Mutex<()> = Mutex::new(());
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = PathBuf::from("target/dream-native-rt");
    std::fs::create_dir_all(&dir)?;
    let archive = dir.join("libdream_rt.a");
    let stamp = dir.join(".stamp");
    let newest = native_runtime_c_files()
        .into_iter()
        .filter_map(|p| std::fs::metadata(p).ok()?.modified().ok())
        .max();
    let stale = match (newest, std::fs::metadata(&stamp).ok().and_then(|m| m.modified().ok())) {
        (Some(src), Some(st)) => src > st,
        _ => true,
    };
    if archive.exists() && !stale {
        return Ok(archive);
    }
    let mut objs = Vec::new();
    for f in native_runtime_c_files() {
        let obj = dir.join(f.file_name().unwrap()).with_extension("o");
        let st = Command::new("cc")
            .args(["-O2", "-std=gnu11", "-pthread", "-w", "-c"])
            .arg(format!("-I{}", native_runtime_include_dir().display()))
            .arg(&f)
            .arg("-o")
            .arg(&obj)
            .status()?;
        if !st.success() {
            return Err(format!("cc -c failed for {}", f.display()).into());
        }
        objs.push(obj);
    }
    let _ = std::fs::remove_file(&archive);
    let mut ar = Command::new("ar");
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

pub fn compile_native_c(c_path: &Path, release: bool) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let bin = c_path.with_extension("bin");
    let rt = runtime_archive()?;
    let mut cmd = Command::new("cc");
    cmd.arg(if release { "-O3" } else { "-O0" });
    cmd.args([
        "-std=gnu11",
        "-pthread",
        "-w",
        "-Wno-unused-function",
        "-Wno-unused-variable",
        "-Wno-unused-parameter",
    ]);
    cmd.arg(format!("-I{}", native_runtime_include_dir().display()));
    cmd.arg(c_path);
    cmd.arg(&rt);
    cmd.args(["-lm", "-lpthread"]);
    if let Some(dir) = libdream_dir() {
        cmd.arg(format!("-L{}", dir.display()));
        cmd.arg("-ldream");
        cmd.arg(format!("-Wl,-rpath,{}", dir.display()));
    }
    cmd.arg("-o").arg(&bin);
    let status = cmd.status()?;
    if !status.success() {
        return Err(format!("cc failed for {}", c_path.display()).into());
    }
    Ok(bin)
}
