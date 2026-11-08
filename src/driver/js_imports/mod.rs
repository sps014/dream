//! Import-triggered source generation. Foreign objects use generated value types containing
//! an owned JS handle, so the ordinary ownership and lowering pipeline remains authoritative.

mod builtins;
mod cache;
mod prepare;
mod tool;

use std::collections::BTreeMap;
use std::io::Error;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsModule {
    pub id: String,
    pub resolved: String,
}

#[derive(Deserialize)]
struct GeneratedModule {
    id: String,
    resolved: String,
    namespace: String,
    #[serde(rename = "rootName")]
    root_name: String,
    files: Vec<GeneratedFile>,
    inputs: Vec<PathBuf>,
    warnings: Vec<String>,
}

#[derive(Deserialize)]
struct GeneratedFile {
    name: String,
    source: String,
}

pub use prepare::prepare;

pub(crate) fn bundle(root: &Path, modules: &[JsModule], target: &str) -> Result<String, Error> {
    let npm = if root.join("dream.toml").is_file() {
        crate::driver::project_manifest::ProjectManifest::load(root)
            .map_err(Error::other)?
            .npm
    } else {
        BTreeMap::new()
    };
    let result = tool::run(
        root,
        &serde_json::json!({"mode":"bundle", "root":root, "npm":npm, "modules":modules, "target":target}),
    )?;
    result["code"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| Error::other("JS bundler returned no module"))
}
