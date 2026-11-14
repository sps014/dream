use crate::manifest::{Dependency, DetailedDependency};
use crate::workspace::Workspace;
use anyhow::{Result, bail};
use std::path::Path;

/// Adds `name` with the source in `spec`; a spec that only names a version (or nothing) becomes a
/// plain registry requirement.
pub fn run(
    start_dir: &Path,
    name: String,
    spec: DetailedDependency,
    dev: bool,
    package: Option<&str>,
) -> Result<()> {
    if spec.path.is_some() && spec.git.is_some() {
        bail!("a dependency cannot have both --path and --git");
    }

    let mut workspace = Workspace::discover_package(start_dir, package)?;

    let dependency = if spec.path.is_some() || spec.git.is_some() {
        Dependency::Detailed(spec)
    } else {
        Dependency::Version(spec.version.unwrap_or_else(|| "*".to_string()))
    };
    dependency.validate()?;

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
