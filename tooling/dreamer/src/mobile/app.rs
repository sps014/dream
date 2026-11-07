//! Mobile canvas apps: compile the guest for one device architecture, package it with the
//! target-built capability libraries, and optionally install, launch, and stream its log.

use super::{Platform, android, devices, doctor, ios, sdk};
use crate::compile_flags::CompileFlags;
use crate::workspace::Workspace;
use anyhow::{Context, Result, bail};
use dream_abi::host_capability::{HostCapability, HostManifest};
use dream_abi::target::TargetSpec;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The compiled guest and the capability libraries the compiler staged beside it.
pub(super) struct Guest {
    pub artifact: PathBuf,
    pub dir: PathBuf,
    pub capabilities: Vec<HostCapability>,
    pub spec: TargetSpec,
}

impl Guest {
    /// Capability libraries and companion files, in the order Android must load them.
    pub fn runtime_files(&self) -> Vec<String> {
        self.capabilities
            .iter()
            .flat_map(|capability| {
                std::iter::once(capability.library_name(&self.spec)).chain(
                    capability
                        .companion_library_names(&self.spec)
                        .iter()
                        .map(|name| name.to_string()),
                )
            })
            .collect()
    }
}

/// Builds the app package; with `launch`, also installs it on a device and streams its log.
pub(crate) fn run(
    workspace: &Workspace,
    flags: &CompileFlags,
    platform: Platform,
    device: Option<&str>,
    launch: bool,
) -> Result<()> {
    let app = workspace
        .manifest
        .app
        .as_ref()
        .context("mobile builds require an [app] table in dream.toml")?;
    doctor::run(platform, false)?;
    let device = if launch {
        Some(select_device(platform, device)?)
    } else {
        device.map(str::to_owned)
    };
    match platform {
        Platform::Android => {
            let root = sdk::android_root()?;
            let triple = match &device {
                Some(serial) => android::device_triple(&root, serial)?,
                None => "aarch64-linux-android",
            };
            let sysroot = sdk::ndk_prebuilt(&sdk::ndk(&root)?)?.join("sysroot");
            let guest = compile(workspace, flags, triple, &sysroot, None)?;
            let apk = android::package(workspace, flags, app, &guest, &root)?;
            println!("built {}", apk.display());
            if let Some(serial) = device {
                android::install_and_launch(&root, &serial, &app.id, &apk)?;
            }
        }
        Platform::Ios => {
            if std::env::consts::ARCH != "aarch64" {
                bail!("iOS simulator apps are built for Apple silicon simulators only");
            }
            let triple = "arm64-apple-ios-simulator";
            let sysroot = sdk::ios_sdk("iphonesimulator")?;
            let guest = compile(workspace, flags, triple, &sysroot, Some(&app.ios_min_os))?;
            let bundle = ios::package(workspace, app, &guest)?;
            println!("built {}", bundle.display());
            if let Some(udid) = device {
                ios::install_and_launch(&udid, &app.id, &bundle)?;
            }
        }
    }
    Ok(())
}

/// The requested device, or the only running one.
fn select_device(platform: Platform, requested: Option<&str>) -> Result<String> {
    let found = devices::discover(platform)?;
    let usable: Vec<_> = found
        .iter()
        .filter(|device| match platform {
            Platform::Android => device.state == "device",
            Platform::Ios => true,
        })
        .collect();
    if let Some(id) = requested {
        return match found.iter().find(|device| device.id == id) {
            Some(device) if usable.iter().any(|usable| usable.id == device.id) => Ok(id.into()),
            Some(device) => bail!("device {id} is {}; it cannot run apps", device.state),
            None => bail!("device {id} not found; list devices with `dreamer devices`"),
        };
    }
    let booted: Vec<_> = usable
        .iter()
        .filter(|device| platform == Platform::Android || device.state == "Booted")
        .collect();
    match booted.as_slice() {
        [device] => Ok(device.id.clone()),
        [] => bail!(
            "no running {} device; start an emulator or simulator, or pass --device ID (see `dreamer devices`)",
            platform.as_str()
        ),
        _ => bail!(
            "several {} devices are running; pass --device ID (see `dreamer devices`)",
            platform.as_str()
        ),
    }
}

fn compile(
    workspace: &Workspace,
    flags: &CompileFlags,
    triple: &str,
    sysroot: &Path,
    min_os: Option<&str>,
) -> Result<Guest> {
    let spec = TargetSpec::parse(triple).map_err(anyhow::Error::msg)?;
    let entry = workspace.compile_root_path()?;
    let stem = entry.file_stem().context("entry file has no name")?;
    let dir = workspace
        .root
        .join("target")
        .join(triple)
        .join(flags.native_artifact_subdir());
    std::fs::create_dir_all(&dir)?;
    let ir = dir.join(stem).with_extension("ll");
    let mut flags = flags.clone();
    flags.native = true;
    flags.relocatable = true;
    let dream = crate::dream_bin::locate()?;
    let mut command = Command::new(&dream);
    flags.apply(&mut command);
    command.args(["--target", triple]);
    if let Some(version) = min_os {
        command.args(["--min-os", version]);
    }
    command
        .arg("-o")
        .arg(&ir)
        .args(["--crate-type", "bin"])
        .arg(&entry)
        .env("DREAM_SYSROOT", sysroot);
    let status = command
        .status()
        .with_context(|| format!("running {}", dream.display()))?;
    if !status.success() {
        bail!("build failed");
    }
    let artifact = ir.with_extension(if spec.is_android() { "so" } else { "bin" });
    if !artifact.is_file() {
        bail!("expected {} after the build", artifact.display());
    }
    let abi = ir.with_extension("abi.json");
    let manifest = HostManifest::parse(
        &std::fs::read_to_string(&abi).with_context(|| format!("reading {}", abi.display()))?,
    )
    .map_err(anyhow::Error::msg)?;
    Ok(Guest {
        artifact,
        dir,
        capabilities: manifest.host_capabilities,
        spec,
    })
}
