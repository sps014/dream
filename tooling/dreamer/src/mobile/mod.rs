//! Mobile canvas apps: SDK inspection, device discovery, packaging, and launch. Nothing here
//! installs SDKs or creates devices; `run` boots an iOS simulator chosen with `--device`.

mod android;
mod app;
mod devices;
mod doctor;
mod ios;
mod sdk;

pub use devices::run as devices;
pub use doctor::run as doctor;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::process::Command;

pub(crate) use app::run as build_app;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Android,
    Ios,
}

impl Platform {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "android" => Ok(Self::Android),
            "ios" => Ok(Self::Ios),
            _ => bail!("unknown mobile platform '{value}'; expected android or ios"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Android => "android",
            Self::Ios => "ios",
        }
    }
}

fn capture(command: &mut Command) -> Result<String> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command
        .output()
        .with_context(|| format!("running {program}"))?;
    if !output.status.success() {
        bail!(
            "{program} failed ({}): {}",
            crate::process_status::describe(&output.status),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout).with_context(|| format!("{program} returned invalid UTF-8"))
}
