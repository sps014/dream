use crate::manifest::{Dependency, DetailedDependency};
use crate::workspace::Workspace;
use anyhow::{bail, Result};
use std::path::Path;

#[allow(clippy::too_many_arguments)]
pub fn run(
    start_dir: &Path,
    name: String,
    version: Option<String>,
    path: Option<String>,
    git: Option<String>,
    tag: Option<String>,
    branch: Option<String>,
    rev: Option<String>,
    dev: bool,
    package: Option<&str>,
) -> Result<()> {
    if path.is_some() && git.is_some() {
        bail!("a dependency cannot have both --path and --git");
    }

    let mut workspace = Workspace::discover_package(start_dir, package)?;

    let dependency = if let Some(path) = path {
        Dependency::Detailed(DetailedDependency {
            path: Some(path),
            version,
            ..Default::default()
        })
    } else if let Some(git) = git {
        Dependency::Detailed(DetailedDependency {
            git: Some(git),
            tag,
            branch,
            rev,
            version,
            ..Default::default()
        })
    } else {
        Dependency::Version(version.unwrap_or_else(|| "*".to_string()))
    };

    let table = if dev {
        &mut workspace.manifest.dev_dependencies
    } else {
        &mut workspace.manifest.dependencies
    };
    table.insert(name.clone(), dependency);
    workspace.save_manifest()?;

    println!(
        "Added '{}' to [{}]",
        name,
        if dev {
            "dev-dependencies"
        } else {
            "dependencies"
        }
    );

    super::install::run(start_dir)
}
