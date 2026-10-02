//! Build and run native Dream binaries against the capability host libraries.

pub(crate) mod bundle;
pub(crate) mod c_link;
pub(crate) mod cc;
pub(crate) mod native_c;
pub(crate) mod pgo;

pub use cc::generator_cache_root;
pub use pgo::Pgo;

use crate::driver::wasm_opt::OptLevel;
use crate::execution::llvm::compile_llvm;
use dream_abi::host_capability::{HostCapability, HostManifest};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn compile_and_capture(
    ll_path: &str,
    opt: OptLevel,
) -> Result<String, Box<dyn std::error::Error>> {
    compile_and_capture_with_env(ll_path, opt, &[])
}

pub fn compile_and_capture_with_env(
    ll_path: &str,
    opt: OptLevel,
    extra_env: &[(&str, &str)],
) -> Result<String, Box<dyn std::error::Error>> {
    compile_and_capture_ex(ll_path, opt, extra_env, &[], None, 8)
}

pub fn compile_and_capture_ex(
    ll_path: &str,
    opt: OptLevel,
    extra_env: &[(&str, &str)],
    extra_args: &[&str],
    stdin: Option<&[u8]>,
    timeout_secs: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    let bin = compile_llvm(Path::new(ll_path), None, opt, false, &Pgo::Off, None, false)?;
    capture_native_bin(&bin, ll_path, extra_env, extra_args, stdin, timeout_secs)
}

/// Runs a built guest with stdout captured; a failing status, a timeout or a non-zero leak count
/// is an error.
pub fn capture_native_bin(
    bin: &Path,
    artifact: &str,
    extra_env: &[(&str, &str)],
    extra_args: &[&str],
    stdin: Option<&[u8]>,
    timeout_secs: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut cmd = Command::new(bin);
    apply_native_run_env(&mut cmd, artifact)?;
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    if std::env::var("DREAM_NATIVE_SANITIZE")
        .ok()
        .is_some_and(|s| s.contains("address") || s.contains("leak"))
        && std::env::var("ASAN_OPTIONS").is_err()
    {
        cmd.env("ASAN_OPTIONS", "detect_leaks=1:halt_on_error=1");
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
    let out = waiter.join().map_err(|_| "native waiter panicked")??;
    if start.elapsed() > limit {
        return Err("native program timed out".into());
    }
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        // The code is spelled out (not just the raw `ExitStatus`) so a trap golden can assert on a
        // chosen exit status portably.
        let code = match out.status.code() {
            Some(c) => format!("exit code {c}"),
            None => format!("status {:?}", out.status),
        };
        return Err(format!("native program failed ({code}): stderr={err} stdout={stdout}").into());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    if let Some(live) = parse_leak_live(&stderr) {
        if live != 0 {
            return Err(format!("guest leak check live={live} (want 0)\n{stderr}").into());
        }
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn parse_leak_live(stderr: &str) -> Option<i32> {
    let marker = "[dream] leak check: live=";
    let i = stderr.find(marker)?;
    let rest = &stderr[i + marker.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// The guest was killed by a signal (`abort()`, SIGSEGV, …). Stderr from the C runtime
/// (the panic message) has already been written to the inherited terminal; callers must
/// not wrap this in another `error: … (status …)` line.
#[derive(Debug)]
pub struct GuestAborted;

impl std::fmt::Display for GuestAborted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native guest aborted")
    }
}

impl std::error::Error for GuestAborted {}

/// Runs the guest and answers its exit status. A non-zero status is a normal outcome — `main` may
/// return `int` or a failing `Result` — so only a crash (killed by a signal, no status at all) is
/// an error here; callers that treat any failure as their own decide that for themselves.
pub fn run_native_bin(
    bin: &Path,
    module: &str,
    extra_args: &[String],
) -> Result<i32, Box<dyn std::error::Error>> {
    let mut cmd = Command::new(bin);
    apply_native_run_env(&mut cmd, module)?;
    cmd.args(extra_args);
    let status = cmd.status()?;
    match status.code() {
        Some(code) => Ok(code),
        None => Err(Box::new(GuestAborted)),
    }
}

pub(crate) fn apply_native_run_env(
    cmd: &mut Command,
    module: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    for (k, v) in native_run_env_pairs(module)? {
        cmd.env(k, v);
    }
    Ok(())
}

pub(crate) fn read_host_capabilities(
    module: &Path,
) -> Result<Vec<HostCapability>, Box<dyn std::error::Error>> {
    let path = module.with_extension("abi.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("reading host inventory {}: {error}", path.display()))?;
    Ok(HostManifest::parse(&text)?.host_capabilities)
}

/// Env vars the native guest needs (`DREAM_NATIVE_MODULE`, dylib search path). Used by
/// `dream run` and the lldb-dap debug adapter.
pub(crate) fn native_run_env_pairs(
    module: &str,
) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let mut out = vec![("DREAM_NATIVE_MODULE".to_string(), module.to_string())];
    let capabilities = read_host_capabilities(Path::new(module))?;
    if let Some(dir) = host_library_dir(&capabilities) {
        let key = if cfg!(target_os = "macos") {
            "DYLD_LIBRARY_PATH"
        } else if cfg!(target_os = "windows") {
            "PATH"
        } else {
            "LD_LIBRARY_PATH"
        };
        let mut paths = dir.display().to_string();
        if let Ok(prev) = std::env::var(key) {
            let sep = if cfg!(target_os = "windows") {
                ';'
            } else {
                ':'
            };
            paths = format!("{paths}{sep}{prev}");
        }
        out.push((key.to_string(), paths));
    }
    Ok(out)
}

fn push_host_library_dir(dirs: &mut Vec<PathBuf>, dir: PathBuf) {
    if !dir.as_os_str().is_empty() && !dirs.iter().any(|d| d == &dir) {
        dirs.push(dir);
    }
}

fn push_exe_parent(dirs: &mut Vec<PathBuf>, exe: &Path) {
    if let Some(p) = exe.parent() {
        push_host_library_dir(dirs, p.to_path_buf());
        if p.file_name().and_then(|s| s.to_str()) == Some("deps") {
            if let Some(parent) = p.parent() {
                push_host_library_dir(dirs, parent.to_path_buf());
            }
        }
    }
}

/// Search order: next to this process (so a dev build links its own newer host symbols rather
/// than an older installed toolchain's), `DREAM_HOME`, `DREAM_BIN`, then `~/.dream/bin`. Never the
/// working directory: planted libraries would otherwise be linked and rpath'd.
/// Development builds use this canonical directory as their absolute rpath; relocatable builds
/// stage the library family into the package instead.
pub(crate) fn host_library_dir(capabilities: &[HostCapability]) -> Option<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(canon) = exe.canonicalize() {
            push_exe_parent(&mut dirs, &canon);
        }
        push_exe_parent(&mut dirs, &exe);
    }
    if let Ok(home) = std::env::var("DREAM_HOME") {
        if !home.is_empty() {
            let home = PathBuf::from(home);
            push_host_library_dir(&mut dirs, home.clone());
            push_host_library_dir(&mut dirs, home.join("bin"));
        }
    }
    if let Ok(bin) = std::env::var("DREAM_BIN") {
        if let Some(p) = Path::new(&bin).parent() {
            push_host_library_dir(&mut dirs, p.to_path_buf());
        }
    }
    if let Ok(user) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        if !user.is_empty() {
            push_host_library_dir(&mut dirs, PathBuf::from(user).join(".dream").join("bin"));
        }
    }
    dirs.into_iter()
        .filter(|d| d.is_absolute())
        .find(|d| {
            capabilities
                .iter()
                .all(|c| d.join(c.library_name()).is_file())
        })
        .and_then(|d| d.canonicalize().ok())
}

fn mtime(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

pub(crate) fn native_bin_fresh(
    bin: &Path,
    module: &Path,
    rt: &Path,
    profile: Option<&Path>,
) -> bool {
    let Ok(meta) = std::fs::metadata(bin) else {
        return false;
    };
    // A link that fails after creating its output leaves an empty file behind, which is newer
    // than every input and would otherwise look like a valid build for as long as the directory
    // survives. For the cached `@json` harness that meant every later compile ran a 0-byte
    // binary and reported the empty output as a generator failure.
    if meta.len() == 0 {
        return false;
    }
    let Some(bin_t) = mtime(bin) else {
        return false;
    };
    mtime(module).is_some_and(|t| t <= bin_t)
        && mtime(rt).is_some_and(|t| t <= bin_t)
        && profile.is_none_or(|p| mtime(p).is_some_and(|t| t <= bin_t))
}
