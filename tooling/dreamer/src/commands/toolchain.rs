use crate::toolchain::{self, Component};
use anyhow::Result;

pub fn install(component: Option<String>) -> Result<()> {
    let components = match component.as_deref() {
        None => toolchain::Component::all().to_vec(),
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
