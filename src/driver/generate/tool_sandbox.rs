use std::io::Read;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Restricts tools to declared scratch inputs, executable resources and system libraries.
pub(super) fn command(
    binary: &Path,
    scratch: &Path,
    resources: Option<&Path>,
) -> Result<Command, String> {
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

/// Pipes are drained concurrently and bounded; the entire process group dies on timeout or exit.
pub(super) fn execute(mut command: Command) -> Result<Output, String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("starting sandboxed tool: {e}"))?;
    #[cfg(unix)]
    let pid = child.id();
    let capture = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(pipe) = pipe {
                std::io::Read::take(pipe, 8 * 1024 * 1024).read_to_end(&mut bytes)?;
            }
            Ok::<_, std::io::Error>(bytes)
        })
    };
    let stdout = capture(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
    );
    let stderr = capture(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
    );
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if start.elapsed() < Duration::from_secs(30) => {
                std::thread::sleep(Duration::from_millis(10))
            }
            Ok(None) => break Err("generator tool exceeded its 30 second timeout".to_string()),
            Err(e) => break Err(format!("waiting for generator tool: {e}")),
        }
    };
    #[cfg(unix)]
    {
        // The group was created above; descendants must not outlive their isolated scratch folder.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    let stdout = stdout
        .join()
        .map_err(|_| "tool stdout capture failed")?
        .map_err(|e| e.to_string())?;
    let stderr = stderr
        .join()
        .map_err(|_| "tool stderr capture failed")?
        .map_err(|e| e.to_string())?;
    Ok(Output {
        status: status?,
        stdout,
        stderr,
    })
}
