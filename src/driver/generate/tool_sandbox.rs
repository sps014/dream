use std::io::Read;
use std::path::Path;
use std::process::Output;
#[cfg(unix)]
use std::process::{Command, Stdio};
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;

#[cfg(windows)]
#[path = "tool_sandbox_windows.rs"]
mod windows;

/// One sandboxed tool process: it writes only `scratch` and reads only system locations, the
/// executable and the tool's install root.
pub(super) struct Invocation<'a> {
    pub binary: &'a Path,
    pub args: &'a [String],
    pub scratch: &'a Path,
    pub resources: Option<&'a Path>,
    pub timeout: Duration,
}

/// Pipes are capped at this many bytes each so a chatty tool cannot exhaust compiler memory.
const PIPE_LIMIT: u64 = 8 * 1024 * 1024;

pub(super) fn run(invocation: &Invocation<'_>) -> Result<Output, String> {
    #[cfg(windows)]
    {
        windows::run(invocation)
    }
    #[cfg(unix)]
    {
        let mut command = command(invocation.binary, invocation.scratch, invocation.resources)?;
        command
            .args(invocation.args)
            .current_dir(invocation.scratch)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", invocation.scratch)
            .env("TMPDIR", invocation.scratch)
            .env("LANG", "C")
            .env("LC_ALL", "C");
        execute(command, invocation.timeout)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = invocation;
        Err("compiler-run generator tools are not supported on this host".into())
    }
}

/// Restricts tools to declared scratch inputs, executable resources and system libraries.
#[cfg(unix)]
fn command(binary: &Path, scratch: &Path, resources: Option<&Path>) -> Result<Command, String> {
    if cfg!(target_os = "macos") {
        let quoted =
            |p: &Path| serde_json::to_string(&p.to_string_lossy()).map_err(|e| e.to_string());
        let resources_rule = resources
            .map(|p| quoted(p).map(|p| format!("(allow file-read* (subpath {p}))")))
            .transpose()?
            .unwrap_or_default();
        let profile = format!(
            "(version 1)(deny default)(allow process*)(allow sysctl-read)(allow mach-lookup)\
             (allow file-read* (literal \"/\") (subpath \"/usr\") (subpath \"/bin\") (subpath \"/System\")\
               (subpath \"/Library\") (subpath \"/private/etc\") (subpath \"/dev\")\
               (literal {}) (subpath {}))\
             (allow file-read-metadata)(allow file-write* (subpath {})){resources_rule}",
            quoted(binary)?,
            quoted(scratch)?,
            quoted(scratch)?,
        );
        let mut cmd = Command::new("/usr/bin/sandbox-exec");
        cmd.args(["-p", &profile]).arg(binary);
        Ok(cmd)
    } else if cfg!(target_os = "linux") {
        let mut cmd = Command::new("bwrap");
        cmd.args(["--unshare-all", "--die-with-parent", "--new-session"]);
        for dir in ["/usr", "/bin", "/lib", "/lib64", "/etc"] {
            if Path::new(dir).exists() {
                cmd.args(["--ro-bind", dir, dir]);
            }
        }
        if let Some(resources) = resources {
            cmd.arg("--ro-bind").arg(resources).arg(resources);
        }
        cmd.args(["--proc", "/proc", "--dev", "/dev"])
            .arg("--ro-bind")
            .arg(binary)
            .arg(binary)
            .arg("--bind")
            .arg(scratch)
            .arg(scratch)
            .arg("--chdir")
            .arg(scratch)
            .arg(binary);
        Ok(cmd)
    } else {
        Err("compiler-run generator tools require macOS sandbox-exec or Linux bubblewrap".into())
    }
}

pub(super) fn spawn_error(error: std::io::Error) -> String {
    if cfg!(target_os = "linux") && error.kind() == std::io::ErrorKind::NotFound {
        return "generator tools run inside bubblewrap, which is not installed; install the \
                `bubblewrap` package (for example `apt install bubblewrap` or \
                `dnf install bubblewrap`) and make sure `bwrap` is on PATH"
            .into();
    }
    format!("starting sandboxed tool: {error}")
}

/// Reads at most [`PIPE_LIMIT`] bytes on its own thread so stdout and stderr drain concurrently.
pub(super) fn capture<R: Read + Send + 'static>(
    pipe: Option<R>,
) -> std::thread::JoinHandle<std::io::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(pipe) = pipe {
            pipe.take(PIPE_LIMIT).read_to_end(&mut bytes)?;
        }
        Ok(bytes)
    })
}

pub(super) fn joined(
    handle: std::thread::JoinHandle<std::io::Result<Vec<u8>>>,
    stream: &str,
) -> Result<Vec<u8>, String> {
    handle
        .join()
        .map_err(|_| format!("tool {stream} capture failed"))?
        .map_err(|e| e.to_string())
}

pub(super) fn timeout_message(timeout: Duration) -> String {
    format!(
        "generator tool exceeded its {} second timeout",
        timeout.as_secs()
    )
}

/// Pipes are drained concurrently and bounded; the entire process group dies on timeout or exit.
#[cfg(unix)]
fn execute(mut command: Command, timeout: Duration) -> Result<Output, String> {
    use std::os::unix::process::CommandExt;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command.spawn().map_err(spawn_error)?;
    let pid = child.id();
    let stdout = capture(child.stdout.take());
    let stderr = capture(child.stderr.take());
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => break Err(timeout_message(timeout)),
            Err(e) => break Err(format!("waiting for generator tool: {e}")),
        }
    };
    // The group was created above; descendants must not outlive their isolated scratch folder.
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
    let stdout = joined(stdout, "stdout")?;
    let stderr = joined(stderr, "stderr")?;
    Ok(Output {
        status: status?,
        stdout,
        stderr,
    })
}
