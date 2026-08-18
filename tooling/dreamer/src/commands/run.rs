use crate::compile_flags::CompileFlags;
use crate::manifest::{resolve_run_target, PackageType, RunTarget};
use crate::workspace::Workspace;
use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

pub fn run(
    start_dir: &Path,
    target: Option<String>,
    release: bool,
    port: Option<u16>,
    extra_args: &[String],
    package: Option<&str>,
) -> Result<()> {
    run_with(
        start_dir,
        target,
        CompileFlags {
            release,
            ..CompileFlags::default()
        },
        port,
        extra_args,
        package,
    )
}

pub fn run_with(
    start_dir: &Path,
    target: Option<String>,
    flags: CompileFlags,
    port: Option<u16>,
    extra_args: &[String],
    package: Option<&str>,
) -> Result<()> {
    super::install::run(start_dir)?;
    let workspace = Workspace::discover_package(start_dir, package)?;
    let pkg = workspace.manifest.package()?;
    if pkg.package_type == PackageType::Lib {
        bail!(
            "package '{}' is type = \"lib\" and is not runnable (use dreamer build to typecheck)",
            pkg.name
        );
    }
    let host = resolve_run_target(&pkg.targets, target.as_deref())?;
    if flags.native_c && host != RunTarget::Native {
        bail!("--native-c / --backend c is only valid with the native host");
    }

    match host {
        RunTarget::Native => run_native(&workspace, &flags, extra_args),
        RunTarget::Node => run_node(&workspace, &flags, extra_args),
        RunTarget::Web => run_web(&workspace, &flags, port),
    }
}

fn run_native(workspace: &Workspace, flags: &CompileFlags, extra_args: &[String]) -> Result<()> {
    let dream_bin = crate::dream_bin::locate()?;
    let entry = workspace.compile_root_path()?;
    let mut cmd = Command::new(&dream_bin);
    flags.apply(&mut cmd);
    cmd.arg("run")
        .arg("--crate-type")
        .arg("bin")
        .arg(&entry)
        .args(extra_args);
    let status = cmd
        .status()
        .map_err(|e| anyhow::anyhow!("running {}: {}", dream_bin.display(), e))?;
    if !status.success() {
        bail!(
            "program exited with a failure (exit code {:?})",
            status.code()
        );
    }
    Ok(())
}

fn run_node(workspace: &Workspace, flags: &CompileFlags, extra_args: &[String]) -> Result<()> {
    let run_mjs = workspace.root.join("run.mjs");
    if !run_mjs.is_file() {
        bail!(
            "missing {}; re-run `dreamer init --runtime node` or add a Node runner that imports \
             the entry's *.node.runtime.js from target/node/",
            run_mjs.display()
        );
    }

    super::build::compile_entry(workspace, flags, Some(RunTarget::Node))?;

    let status = Command::new("node")
        .arg(&run_mjs)
        .args(extra_args)
        .current_dir(&workspace.root)
        .status()
        .map_err(|e| anyhow::anyhow!("running node: {}", e))?;
    if !status.success() {
        bail!("node exited with a failure (exit code {:?})", status.code());
    }
    Ok(())
}

fn run_web(workspace: &Workspace, flags: &CompileFlags, port: Option<u16>) -> Result<()> {
    let index = workspace.root.join("index.html");
    if !index.is_file() {
        bail!(
            "missing {}; re-run `dreamer init --runtime web` or add an index.html that imports \
             the entry's *.web.runtime.js from target/web/",
            index.display()
        );
    }

    super::build::compile_entry(workspace, flags, Some(RunTarget::Web))?;
    let port = port.unwrap_or(crate::serve::DEFAULT_WEB_PORT);
    crate::serve::serve_project(&workspace.root, port)
}
