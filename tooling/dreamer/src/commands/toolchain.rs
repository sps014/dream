use crate::toolchain::{self, Component};
use anyhow::Result;

pub fn install(component: Option<String>) -> Result<()> {
    let components = match component.as_deref() {
        None => toolchain::available_components()?,
        Some(name) => vec![Component::parse_name(name)?],
    };
    toolchain::install(&components)
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
