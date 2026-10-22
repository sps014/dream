//! Build and run native Dream binaries against the capability host libraries.

pub(crate) mod bundle;
pub(crate) mod c_link;
pub(crate) mod capability_abi;
pub(crate) mod cc;
pub mod exit_status;
pub(crate) mod native_c;
pub(crate) mod pgo;
pub(crate) mod runtime_manifest;

pub use exit_status::Crash;
pub use pgo::Pgo;

use crate::driver::wasm_opt::OptLevel;
use crate::execution::llvm::compile_llvm;
use dream_abi::host_capability::{HostCapability, HostManifest};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn compile_and_capture(
    config: &std::sync::Arc<crate::driver::toolchain::ToolchainConfig>,
    ll_path: &str,
    opt: OptLevel,
) -> Result<String, Box<dyn std::error::Error>> {
    compile_and_capture_with_env(config, ll_path, opt, &[])
}

pub fn compile_and_capture_with_env(
    config: &std::sync::Arc<crate::driver::toolchain::ToolchainConfig>,
    ll_path: &str,
    opt: OptLevel,
    extra_env: &[(&str, &str)],
) -> Result<String, Box<dyn std::error::Error>> {
    compile_and_capture_ex(config, ll_path, opt, extra_env, &[], None, 8)
}

/// Debug builds only exist at `-O0`, so `O0` links the debug runtime (heap counters and the
/// exit-time leak report) and every other level links the release runtime.
pub fn compile_and_capture_ex(
    config: &std::sync::Arc<crate::driver::toolchain::ToolchainConfig>,
    ll_path: &str,
    opt: OptLevel,
    extra_env: &[(&str, &str)],
    extra_args: &[&str],
    stdin: Option<&[u8]>,
    timeout_secs: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    let bin = compile_llvm(
        config,
        Path::new(ll_path),
        crate::execution::llvm::NativeBuildOptions {
            target: dream_abi::target::TargetSpec::host(),
            opt_ll: None,
            opt_remarks: None,
            opt,
            debug: false,
            profile: dream_abi::profile::CompileProfile::from_release(opt != OptLevel::O0),
            pgo: &Pgo::Off,
            icon: None,
            relocatable: false,
            output_kind: crate::driver::output::OutputKind::Executable,
        },
    )?;
    capture_native_bin(
        config,
        &bin,
        ll_path,
        extra_env,
        extra_args,
        stdin,
        timeout_secs,
    )
}

/// Runs a built guest with stdout captured; a failing status, a timeout or a non-zero leak count
/// is an error.
pub fn capture_native_bin(
    config: &std::sync::Arc<crate::driver::toolchain::ToolchainConfig>,
    bin: &Path,
    artifact: &str,
    extra_env: &[(&str, &str)],
    extra_args: &[&str],
    stdin: Option<&[u8]>,
    timeout_secs: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut cmd = Command::new(bin);
    apply_native_run_env(config, &mut cmd, artifact)?;
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    if config.native_sanitize.as_ref().is_some_and(|s| {
        s.to_string_lossy().contains("address") || s.to_string_lossy().contains("leak")
    }) && config.asan_options.is_none()
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
    if let Some(bytes) = stdin
        && let Some(mut sin) = child.stdin.take()
    {
        let _ = std::io::Write::write_all(&mut sin, bytes);
    }
    let pid = child.id();
    let waiter = std::thread::spawn(move || child.wait_with_output());
    let limit = Duration::from_secs(timeout_secs);
    let start = Instant::now();
    while !waiter.is_finished() {
        if start.elapsed() > limit {
            if cfg!(windows) {
                let _ = Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .status();
            } else {
                let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            }
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
    if let Some(live) = parse_leak_live(&stderr)
        && live != 0
    {
        return Err(format!("guest leak check live={live} (want 0)\n{stderr}").into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn parse_leak_live(stderr: &str) -> Option<u64> {
    let marker = "[dream] leak check: live=";
    let i = stderr.find(marker)?;
    let rest = &stderr[i + marker.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// The guest crashed: a Unix signal (`abort()`, SIGSEGV, …) or a Windows exception. A panic
/// message from the C runtime, when there is one, is already on the inherited stderr.
#[derive(Debug)]
pub struct GuestAborted(pub Crash);

impl std::fmt::Display for GuestAborted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "guest crashed: {}", self.0)
    }
}

impl std::error::Error for GuestAborted {}

/// Runs the guest and answers its exit status. A non-zero status is a normal outcome — `main` may
/// return `int` or a failing `Result` — so only a crash is an error here; callers that treat any
/// failure as their own decide that for themselves.
/// `cwd` is the directory the guest starts in (the build output when the package has assets).
pub fn run_native_bin(
    config: &std::sync::Arc<crate::driver::toolchain::ToolchainConfig>,
    bin: &Path,
    module: &str,
    extra_args: &[String],
    cwd: Option<&Path>,
) -> Result<i32, Box<dyn std::error::Error>> {
    let mut cmd = native_command(config, bin, module, cwd)?;
    cmd.args(extra_args);
    let status = cmd.status()?;
    if let Some(crash) = Crash::of(status) {
        return Err(Box::new(GuestAborted(crash)));
    }
    Ok(status.code().unwrap_or(1))
}

/// A guest command with its run environment. Paths are made absolute before a `cwd` change so the
/// binary and its module sidecar still resolve from the new directory.
pub fn native_command(
    config: &std::sync::Arc<crate::driver::toolchain::ToolchainConfig>,
    bin: &Path,
    module: &str,
    cwd: Option<&Path>,
) -> Result<Command, Box<dyn std::error::Error>> {
    let Some(cwd) = cwd else {
        let mut cmd = Command::new(bin);
        apply_native_run_env(config, &mut cmd, module)?;
        return Ok(cmd);
    };
    let bin = dunce::canonicalize(bin)?;
    let module = std::path::absolute(module)?;
    let mut cmd = Command::new(bin);
    apply_native_run_env(config, &mut cmd, &module.to_string_lossy())?;
    cmd.current_dir(cwd);
    Ok(cmd)
}

pub(crate) fn apply_native_run_env(
    config: &std::sync::Arc<crate::driver::toolchain::ToolchainConfig>,
    cmd: &mut Command,
    module: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    for (k, v) in native_run_env_pairs(config, module)? {
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
    config: &std::sync::Arc<crate::driver::toolchain::ToolchainConfig>,
    module: &str,
) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let mut out = vec![("DREAM_NATIVE_MODULE".to_string(), module.to_string())];
    let capabilities = read_host_capabilities(Path::new(module))?;
    if let Some(dir) = host_library_dir(
        config,
        &capabilities,
        &dream_abi::target::TargetSpec::host(),
    ) {
        let key = crate::driver::toolchain::ToolchainConfig::loader_path_key();
        let mut paths = dir.display().to_string();
        if let Some(prev) = &config.loader_path {
            let sep = if cfg!(target_os = "windows") {
                ';'
            } else {
                ':'
            };
            paths = format!("{paths}{sep}{}", prev.to_string_lossy());
        }
        out.push((key.to_string(), paths));
    }
    Ok(out)
}

/// Capability hosts are searched next to the compiler, then in configured install roots, never the cwd.
pub(crate) fn host_library_dir(
    config: &crate::driver::toolchain::ToolchainConfig,
    capabilities: &[HostCapability],
    spec: &dream_abi::target::TargetSpec,
) -> Option<PathBuf> {
    if capabilities.is_empty() {
        return None;
    }
    let directories = if spec.can_link_on_host() {
        config.host_library_dirs()
    } else {
        vec![config.targets.join(spec.triple.to_string()).join("lib")]
    };
    directories
        .into_iter()
        .find(|d| {
            capabilities
                .iter()
                .all(|c| d.join(c.library_name(spec)).is_file())
        })
        .and_then(|d| dunce::canonicalize(d).ok())
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

#[cfg(test)]
mod tests {
    #[test]
    fn leak_live_parses_counts_past_i32() {
        let stderr = "[dream] leak check: live=3000000000 total_allocations=3000000001 live_bytes=96000000000\n";
        assert_eq!(super::parse_leak_live(stderr), Some(3_000_000_000));
        assert_eq!(super::parse_leak_live("no report"), None);
    }
}
