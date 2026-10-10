use crate::toolchain::{self, Component};
use anyhow::Result;

pub fn install(component: Option<String>) -> Result<()> {
    let components = match component.as_deref() {
        None => toolchain::available_components()?,
        Some(name) => vec![Component::parse_name(name)?],
    };
    toolchain::install(&components)?;
    prewarm_generators();
    Ok(())
}

/// Builds the std generator executables so the first `@json` build only launches one. Best
/// effort: a toolchain that cannot build them yet (no LLVM) still installed fine.
fn prewarm_generators() {
    let Ok(dream) = crate::dream_bin::locate() else {
        return;
    };
    let ok = std::process::Command::new(&dream)
        .args(["generate", "--prewarm"])
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        eprintln!(
            "note: std generators were not prebuilt; the first build that uses one builds it"
        );
    }
}

pub fn list() -> Result<()> {
    toolchain::list()
}

pub fn uninstall(component: String) -> Result<()> {
    toolchain::uninstall(Component::parse_name(&component)?)
}

pub fn doctor(target: Option<&str>, json: bool) -> Result<()> {
    let mut command = std::process::Command::new(crate::dream_bin::locate()?);
    command.arg("toolchain-doctor");
    if let Some(target) = target {
        command.args(["--target", target]);
    }
    if json {
        command.arg("--json");
    }
    if !command.status()?.success() {
        anyhow::bail!("toolchain diagnosis found missing requirements");
    }
    Ok(())
}
