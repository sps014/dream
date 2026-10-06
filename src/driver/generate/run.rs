//! Runs a cached generator executable on one snapshot and reads back its structured result.

use super::exe::GenExe;
use super::model::GenResult;
use crate::driver::toolchain::ToolchainConfig;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const DEFAULT_TIMEOUT_SECS: u64 = 300;

/// Scratch snapshot/result files for one run, removed on drop.
struct RunFiles {
    snapshot: PathBuf,
    result: PathBuf,
}

impl RunFiles {
    fn new(cache_root: &Path, generator: &str) -> std::io::Result<Self> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = cache_root.join("runs");
        std::fs::create_dir_all(&dir)?;
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let stem = format!("{generator}-{}-{n}", std::process::id());
        Ok(RunFiles {
            snapshot: dir.join(format!("{stem}.snapshot.json")),
            result: dir.join(format!("{stem}.result.json")),
        })
    }
}

impl Drop for RunFiles {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.snapshot);
        let _ = std::fs::remove_file(&self.result);
    }
}

fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// The argv a generator executable takes; shared with the debug adapter.
pub fn exe_args(generator: &str, snapshot: &Path, result: &Path) -> Vec<String> {
    vec![
        "--generator".to_string(),
        generator.to_string(),
        "--snapshot".to_string(),
        snapshot.to_string_lossy().into_owned(),
        "--result".to_string(),
        result.to_string_lossy().into_owned(),
    ]
}

pub fn run(
    config: &Arc<ToolchainConfig>,
    exe: &GenExe,
    generator: &str,
    snapshot_json: &str,
    timeout_secs: u64,
) -> Result<GenResult, String> {
    let files = RunFiles::new(&config.generator_cache_root(), generator)
        .map_err(|e| format!("generator '{generator}': scratch files: {e}"))?;
    std::fs::write(&files.snapshot, snapshot_json)
        .map_err(|e| format!("generator '{generator}': write snapshot: {e}"))?;
    let mut cmd = Command::new(&exe.bin);
    let module = exe.ll.to_string_lossy().into_owned();
    for (k, v) in crate::execution::native::native_run_env_pairs(config, &module)
        .map_err(|e| format!("generator '{generator}': {e}"))?
    {
        cmd.env(k, v);
    }
    cmd.args(exe_args(generator, &files.snapshot, &files.result))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = cmd.spawn().map_err(|e| {
        format!(
            "generator '{generator}': cannot start {}: {e}",
            exe.bin.display()
        )
    })?;
    let pid = child.id();
    let waiter = std::thread::spawn(move || child.wait_with_output());
    let limit = Duration::from_secs(timeout_secs);
    let start = Instant::now();
    let mut timed_out = false;
    while !waiter.is_finished() {
        if start.elapsed() > limit {
            timed_out = true;
            kill(pid);
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let out = waiter
        .join()
        .map_err(|_| format!("generator '{generator}': waiter panicked"))?
        .map_err(|e| format!("generator '{generator}': {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let debug_hint = || {
        let kept = keep_failed_run(
            &config.generator_cache_root(),
            exe,
            generator,
            snapshot_json,
            timeout_secs,
        );
        format!(
            "rerun it under the debugger with `dream debug-adapter --generator {generator} --snapshot {}` \
             (or without one: `dream generate --replay {}`)",
            kept.display(),
            kept.display()
        )
    };
    if timed_out {
        return Err(format!(
            "generator '{generator}' timed out after {timeout_secs}s; {}",
            debug_hint()
        ));
    }
    if !out.status.success() {
        let status = match out.status.code() {
            Some(c) => format!("exit code {c}"),
            None => format!("{}", out.status),
        };
        let shown = tail(&stderr, 20);
        let shown = if shown.trim().is_empty() {
            tail(&stdout, 20)
        } else {
            shown
        };
        return Err(format!(
            "generator '{generator}' crashed ({status}):\n{shown}\n{}",
            debug_hint()
        ));
    }
    let text = std::fs::read_to_string(&files.result).map_err(|e| {
        format!(
            "generator '{generator}' wrote no result ({e}); {}",
            debug_hint()
        )
    })?;
    let mut result: GenResult = serde_json::from_str(&text)
        .map_err(|e| format!("generator '{generator}' wrote a malformed result: {e}"))?;
    result
        .logs
        .extend(stdout.lines().filter(|l| !l.is_empty()).map(str::to_string));
    Ok(result)
}

/// What a capture directory records next to `snapshot.json` so `--replay` and the debug adapter
/// can rerun the generator without the front end.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Capture {
    pub generator: String,
    pub bin: PathBuf,
    pub ll: PathBuf,
    pub timeout_secs: u64,
}

pub const CAPTURE_FILE: &str = "capture.json";
pub const SNAPSHOT_FILE: &str = "snapshot.json";

/// Writes a capture directory for one run.
pub fn write_capture(
    dir: &Path,
    exe: &GenExe,
    generator: &str,
    snapshot_json: &str,
    timeout_secs: u64,
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let meta = Capture {
        generator: generator.to_string(),
        bin: exe.bin.clone(),
        ll: exe.ll.clone(),
        timeout_secs,
    };
    let write = |file: &str, text: &str| {
        let path = dir.join(file);
        std::fs::write(&path, text).map_err(|e| format!("write {}: {e}", path.display()))
    };
    write(SNAPSHOT_FILE, snapshot_json)?;
    write(
        CAPTURE_FILE,
        &serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?,
    )
}

/// Keeps a failed run as a capture directory (the scratch files are removed on drop) so the
/// hint's commands reproduce exactly that run.
fn keep_failed_run(
    cache_root: &Path,
    exe: &GenExe,
    generator: &str,
    snapshot_json: &str,
    timeout_secs: u64,
) -> PathBuf {
    let dir = cache_root.join("failed").join(generator);
    let _ = write_capture(&dir, exe, generator, snapshot_json, timeout_secs);
    dir
}

fn kill(pid: u32) {
    if cfg!(windows) {
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .status();
    } else {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
    }
}
