use crate::lockfile::Lockfile;
use crate::manifest::Manifest;
use crate::resolver;
use crate::workspace::{self, Workspace, PACKAGES_DIR_NAME};
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::Path;

pub fn run(start_dir: &Path) -> Result<()> {
    let (install_root, members) = workspace::discover_install_root(start_dir)?;

    let lock_path = install_root.join(crate::lockfile::LOCKFILE_FILE_NAME);
    let existing_lock = Lockfile::load_if_exists(&lock_path)?;
    let preferred: BTreeMap<String, String> = existing_lock
        .map(|lock| {
            lock.packages
                .into_iter()
                .map(|p| (p.name, p.version))
                .collect()
        })
        .unwrap_or_default();

    let resolved = resolver::resolve_many(&members, true, &preferred)?;

    // Use a Workspace shell so install materializes into install_root and symlinks members.
    let shell = install_shell(&install_root, &members)?;
    let lockfile = shell.install(&resolved)?;
    lockfile.save(&lock_path)?;

    println!(
        "Installed {} package(s) into {}",
        lockfile.packages.len(),
        install_root.join(PACKAGES_DIR_NAME).display()
    );
    for pkg in &lockfile.packages {
        println!("  {} {}", pkg.name, pkg.version);
    }
    Ok(())
}

fn install_shell(install_root: &Path, members: &[(std::path::PathBuf, Manifest)]) -> Result<Workspace> {
    let root_manifest = Manifest::load(&install_root.join(crate::manifest::MANIFEST_FILE_NAME))?;
    if root_manifest.is_workspace_root() {
        // Prefer any member as the Workspace.root for symlink logic; install_root via workspace_root.
        let (member_dir, member_manifest) = members
            .iter()
            .find(|(dir, _)| dir.as_path() != install_root)
            .cloned()
            .or_else(|| members.first().cloned())
            .ok_or_else(|| anyhow::anyhow!("workspace has no members to install"))?;
        Ok(Workspace {
            root: member_dir,
            manifest: member_manifest,
            workspace_root: Some(install_root.to_path_buf()),
        })
    } else {
        let (dir, manifest) = members
            .first()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no package to install"))?;
        Ok(Workspace {
            root: dir,
            manifest,
            workspace_root: None,
        })
    }
}
