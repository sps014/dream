use crate::resolver;
use crate::workspace::{self, Workspace};
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::Path;

/// Re-resolves ignoring the existing lock's pinned versions (except for `name`, when given, which
/// updates just that one package and keeps every other pin as-is).
pub fn run(start_dir: &Path, name: Option<String>) -> Result<()> {
    let (install_root, members) = workspace::discover_install_root(start_dir)?;
    let lock_path = install_root.join(crate::lockfile::LOCKFILE_FILE_NAME);

    let preferred: BTreeMap<String, String> = match (
        &name,
        crate::lockfile::Lockfile::load_if_exists(&lock_path)?,
    ) {
        (Some(keep_others_pinned), Some(lock)) => lock
            .packages
            .into_iter()
            .filter(|p| &p.name != keep_others_pinned)
            .map(|p| (p.name, p.version))
            .collect(),
        _ => BTreeMap::new(),
    };

    let resolved = resolver::resolve_many(&members, true, &preferred)?;
    let shell = update_shell(&install_root, &members)?;
    let lockfile = shell.install(&resolved)?;
    lockfile.save(&lock_path)?;

    match name {
        Some(name) => println!(
            "Updated '{}' (and re-resolved anything affected by it)",
            name
        ),
        None => println!("Updated all dependencies to the latest compatible versions"),
    }
    for pkg in &lockfile.packages {
        println!("  {} {}", pkg.name, pkg.version);
    }
    Ok(())
}

fn update_shell(
    install_root: &Path,
    members: &[(std::path::PathBuf, crate::manifest::Manifest)],
) -> Result<Workspace> {
    let root_manifest =
        crate::manifest::Manifest::load(&install_root.join(crate::manifest::MANIFEST_FILE_NAME))?;
    if root_manifest.is_workspace_root() {
        let (member_dir, member_manifest) = members
            .iter()
            .find(|(dir, _)| dir.as_path() != install_root)
            .cloned()
            .or_else(|| members.first().cloned())
            .ok_or_else(|| anyhow::anyhow!("workspace has no members"))?;
        Ok(Workspace {
            root: member_dir,
            manifest: member_manifest,
            workspace_root: Some(install_root.to_path_buf()),
        })
    } else {
        let (dir, manifest) = members
            .first()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no package to update"))?;
        Ok(Workspace {
            root: dir,
            manifest,
            workspace_root: None,
        })
    }
}
