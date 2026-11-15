use crate::fetch;
use crate::registry::{
    IndexDependency, IndexEntry, MAX_TARBALL_BYTES, checksum, open_registry_with_token,
};
use crate::workspace::Workspace;
use anyhow::{Context, Result, bail};
use std::path::Path;

pub fn run(
    start_dir: &Path,
    registry_url: Option<String>,
    token: Option<String>,
    package: Option<&str>,
) -> Result<()> {
    if package.is_none()
        && crate::manifest::Manifest::find_package_root(start_dir).is_none()
        && let Some(ws_root) = crate::manifest::Manifest::find_workspace_root(start_dir)
    {
        return publish_workspace(&ws_root, registry_url, token);
    }
    let workspace = Workspace::discover_package(start_dir, package)?;
    publish_one(&workspace, registry_url, token)
}

/// Publishes every non-test member, dependencies before the packages that require them.
/// A version that is already in the registry is left as-is so a later package can still refer to it.
fn publish_workspace(
    ws_root: &Path,
    registry_url: Option<String>,
    token: Option<String>,
) -> Result<()> {
    let members = crate::workspace::publish_order(ws_root)?;
    if members.is_empty() {
        bail!("workspace has no publishable packages");
    }
    let names: Vec<&str> = members
        .iter()
        .map(|member| member.manifest.package().map(|pkg| pkg.name.as_str()))
        .collect::<Result<_>>()?;
    println!(
        "Publishing {} package(s), dependencies first: {}",
        names.len(),
        names.join(", ")
    );
    for member in &members {
        match publish_one(member, registry_url.clone(), token.clone()) {
            Ok(()) => {}
            Err(error) if already_published(&error) => {
                println!("{error}");
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn already_published(error: &anyhow::Error) -> bool {
    error.to_string().contains("is already published")
}

fn publish_one(
    workspace: &Workspace,
    registry_url: Option<String>,
    token: Option<String>,
) -> Result<()> {
    workspace.manifest.validate()?;
    let pkg = workspace.manifest.package()?;
    if pkg.package_type == crate::manifest::PackageType::Test {
        bail!(
            "package '{}' is type = \"test\"; test packages are never published",
            pkg.name
        );
    }

    for (dep_name, dep) in &workspace.manifest.dependencies {
        if dep.is_path_only() {
            bail!(
                "cannot publish '{}': dependency '{}' is path-only. Add a version for the \
                 registry, e.g. {dep_name} = {{ version = \"{}\", path = \"...\" }}",
                pkg.name,
                dep_name,
                "0.1.0"
            );
        }
    }

    let url = match registry_url {
        Some(u) => u,
        None => workspace
            .manifest
            .registry_url(None)
            .context("no registry configured (pass --registry or set [registries] default)")?,
    };

    let tarball_path = fetch::cache_dir().join(format!("{}-{}.tar.gz", pkg.name, pkg.version));
    let bytes = fetch::package_project(&workspace.root, &tarball_path)?;
    if bytes.len() > MAX_TARBALL_BYTES {
        bail!(
            "package tarball is {} bytes; registry limit is {} bytes (10 MiB)",
            bytes.len(),
            MAX_TARBALL_BYTES
        );
    }
    let cksum = checksum::sha256_of(&bytes);

    let deps: Vec<IndexDependency> = workspace
        .manifest
        .dependencies
        .iter()
        .filter_map(|(name, dep)| {
            dep.version_req().map(|req| IndexDependency {
                name: name.clone(),
                req: req.to_string(),
            })
        })
        .collect();

    let entry = IndexEntry {
        name: pkg.name.clone(),
        vers: pkg.version.clone(),
        deps,
        cksum,
        tarball: format!("dl/{}/{}-{}.tar.gz", pkg.name, pkg.name, pkg.version),
        description: pkg.description.clone(),
        authors: pkg.authors.clone(),
        license: pkg.license.clone(),
        edition: pkg.edition.clone(),
        package_type: Some(pkg.package_type.as_str().to_string()),
        targets: pkg.targets.clone(),
        readme: fetch::find_readme_name(&workspace.root),
        keywords: pkg.keywords.clone(),
    };

    let client = open_registry_with_token(&url, token)?;
    client.publish(&entry, &tarball_path)?;

    println!(
        "Published {} {} to {}",
        entry.name,
        entry.vers,
        client.base_url()
    );
    Ok(())
}
