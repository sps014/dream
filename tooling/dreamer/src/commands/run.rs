use crate::compile_flags::CompileFlags;
use crate::manifest::{PackageType, RunTarget};
use crate::workspace::Workspace;
use anyhow::{Result, bail};
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
        Selection {
            target,
            device: None,
        },
        CompileFlags {
            compile_profile: dream_abi::profile::CompileProfile::from_release(release),
            ..CompileFlags::default()
        },
        port,
        extra_args,
        package,
    )
}

/// The host from `--target` and, for Android and iOS, the device from `--device`.
pub struct Selection {
    pub target: Option<String>,
    pub device: Option<String>,
}

pub fn run_with(
    start_dir: &Path,
    selection: Selection,
    flags: CompileFlags,
    port: Option<u16>,
    extra_args: &[String],
    package: Option<&str>,
) -> Result<()> {
    let Selection { target, device } = selection;
    super::install::run(start_dir)?;
    let workspace = Workspace::discover_package(start_dir, package)?;
    let pkg = workspace.manifest.package()?;
    match pkg.package_type {
        PackageType::Lib => bail!(
            "package '{}' is type = \"lib\" and is not runnable (use dreamer build to typecheck)",
            pkg.name
        ),
        PackageType::Test => bail!(
            "package '{}' is type = \"test\"; run its tests with `dreamer test`",
            pkg.name
        ),
        PackageType::Bin => {}
    }
    let host = workspace.manifest.run_target(target.as_deref())?;
    if !flags.native && !matches!(host, RunTarget::Web | RunTarget::Node) {
        bail!("--wasm is only valid with web/node hosts (native run builds a host binary)");
    }
    let mobile = match host {
        RunTarget::Android => Some(crate::mobile::Platform::Android),
        RunTarget::Ios => Some(crate::mobile::Platform::Ios),
        _ => None,
    };
    match mobile {
        Some(platform) => {
            if !extra_args.is_empty() {
                bail!("mobile apps do not take command-line arguments");
            }
            crate::mobile::build_app(&workspace, &flags, platform, device.as_deref(), true)
        }
        None if device.is_some() => {
            bail!("--device selects an Android or iOS device; use it with --target android or ios")
        }
        None if host == RunTarget::Node => run_node(&workspace, &flags, extra_args),
        None if host == RunTarget::Web => run_web(&workspace, &flags, port),
        None => run_native(&workspace, &flags, extra_args),
    }
}

fn run_native(workspace: &Workspace, flags: &CompileFlags, extra_args: &[String]) -> Result<()> {
    let dream_bin = crate::dream_bin::locate()?;
    let entry = workspace.compile_root_path()?;
    let mut cmd = Command::new(&dream_bin);
    flags.apply(&mut cmd);
    if let Some(icon) = crate::app_icon::resolve(workspace)? {
        cmd.arg("--icon").arg(icon);
    }
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
            "program exited with a failure ({})",
            crate::process_status::describe(&status)
        );
    }
    Ok(())
}

fn run_node(workspace: &Workspace, flags: &CompileFlags, extra_args: &[String]) -> Result<()> {
    let stem = crate::artifact_alias::entry_stem(&workspace.compile_root_path()?)?;
    crate::artifact_alias::ensure_node_runner(&workspace.root, &stem)?;
    let run_mjs = workspace.root.join("run.mjs");

    super::build::compile_entry(workspace, flags, Some(RunTarget::Node))?;

    let cwd = if workspace.manifest.package()?.assets.is_empty() {
        workspace.root.clone()
    } else {
        workspace.root.join("target").join("node")
    };
    let status = Command::new("node")
        .arg(&run_mjs)
        .args(extra_args)
        .current_dir(cwd)
        .status()
        .map_err(|e| anyhow::anyhow!("running node: {}", e))?;
    if !status.success() {
        bail!(
            "node exited with a failure ({})",
            crate::process_status::describe(&status)
        );
    }
    Ok(())
}

fn run_web(workspace: &Workspace, flags: &CompileFlags, port: Option<u16>) -> Result<()> {
    let index = workspace.root.join("index.html");
    if !index.is_file() {
        bail!(
            "missing {}; add an index.html like the `dreamer init web` template that imports \
             the entry's *.web.runtime.js from target/web/",
            index.display()
        );
    }

    super::build::compile_entry(workspace, flags, Some(RunTarget::Web))?;
    let port = port.unwrap_or(crate::serve::DEFAULT_WEB_PORT);
    crate::serve::serve_project(&workspace.root, port)
}
