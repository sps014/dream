//! Project discovery (`dream.toml` lookup) and `dream_packages/` materialization: takes a
//! resolved dependency graph and lays each package out on disk where the compiler's import
//! resolution (`src/driver/source_loader.rs`) expects to find it.
//!
//! A `[workspace]` root shares one `dream.lock` + `dream_packages/` across members; each member
//! gets a `dream_packages` symlink back to that root so LSP/compiler discovery stays unchanged.

use crate::fetch;
use crate::lockfile::{LockedPackage, Lockfile, LOCKFILE_FILE_NAME};
use crate::manifest::{import_segment, Manifest, PackageType, MANIFEST_FILE_NAME};
use crate::registry::open_registry;
use crate::resolver::{ResolvedPackage, ResolvedSource};
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The directory dependency sources are materialized into, sitting next to `dream.toml` (or the
/// workspace root). Never committed to version control (see `dreamer init`'s generated
/// `.gitignore`) since its contents are fully reproducible from `dream.toml` + `dream.lock`.
pub const PACKAGES_DIR_NAME: &str = "dream_packages";

pub struct Workspace {
    /// Directory of this package's `dream.toml`.
    pub root: PathBuf,
    pub manifest: Manifest,
    /// When this package belongs to a `[workspace]`, the workspace root (lockfile + shared
    /// `dream_packages/`). `None` for a standalone package.
    pub workspace_root: Option<PathBuf>,
}

impl Workspace {
    /// Discover the package enclosing `start_dir`. At a virtual workspace root this errors —
    /// use [`Self::discover_package`] with `-p`.
    pub fn discover(start_dir: &Path) -> Result<Workspace> {
        Self::discover_package(start_dir, None)
    }

    /// Discover a package: optional `-p <name>` selects a workspace member; otherwise the nearest
    /// `[package]` root is used (error if standing on a virtual workspace root with no `-p`).
    pub fn discover_package(start_dir: &Path, package_name: Option<&str>) -> Result<Workspace> {
        if let Some(name) = package_name {
            return Self::discover_named(start_dir, name);
        }

        let package_root = Manifest::find_package_root(start_dir).with_context(|| {
            if Manifest::find_workspace_root(start_dir).is_some() {
                format!(
                    "at a workspace root with no [package]; pass -p <name> to select a member \
                     (looked from {})",
                    start_dir.display()
                )
            } else {
                format!(
                    "no {} with [package] found in {} or any parent directory",
                    MANIFEST_FILE_NAME,
                    start_dir.display()
                )
            }
        })?;
        Self::from_package_dir(&package_root)
    }

    fn discover_named(start_dir: &Path, name: &str) -> Result<Workspace> {
        let ws_root = Manifest::find_workspace_root(start_dir).with_context(|| {
            format!(
                "-p {name} requires a [workspace] root above {}",
                start_dir.display()
            )
        })?;
        let members = load_member_dirs(&ws_root)?;
        for member_dir in members {
            let manifest = Manifest::load(&member_dir.join(MANIFEST_FILE_NAME))?;
            let pkg = manifest.package()?;
            if pkg.name == name {
                return Ok(Workspace {
                    root: member_dir,
                    manifest,
                    workspace_root: Some(ws_root),
                });
            }
        }
        let names = member_package_names(&ws_root)?;
        bail!(
            "no workspace member named '{name}' (members: {})",
            names.join(", ")
        );
    }

    fn from_package_dir(package_root: &Path) -> Result<Workspace> {
        let manifest = Manifest::load(&package_root.join(MANIFEST_FILE_NAME))?;
        let _ = manifest.package()?;
        let workspace_root = Manifest::find_workspace_root(package_root).and_then(|ws| {
            // Only attach when this package is listed as a member (or is the root package).
            if package_is_workspace_member(&ws, package_root).unwrap_or(false) {
                Some(ws)
            } else {
                None
            }
        });
        Ok(Workspace {
            root: package_root.to_path_buf(),
            manifest,
            workspace_root,
        })
    }

    /// Directory that owns `dream.lock` and the shared `dream_packages/` (workspace root when set).
    pub fn install_root(&self) -> &Path {
        self.workspace_root.as_deref().unwrap_or(&self.root)
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.root.join(MANIFEST_FILE_NAME)
    }

    pub fn lockfile_path(&self) -> PathBuf {
        self.install_root().join(LOCKFILE_FILE_NAME)
    }

    pub fn packages_dir(&self) -> PathBuf {
        self.install_root().join(PACKAGES_DIR_NAME)
    }

    /// Source file handed to the compiler for build/run/pack.
    /// - `bin`: `[package].entry`
    /// - `lib`: conventional `src/<import_segment>.dream`
    pub fn compile_root_path(&self) -> Result<PathBuf> {
        let pkg = self.manifest.package()?;
        match pkg.package_type {
            PackageType::Bin => {
                let entry = pkg
                    .entry
                    .as_deref()
                    .filter(|e| !e.trim().is_empty())
                    .with_context(|| format!("package '{}' is missing entry", pkg.name))?;
                Ok(self.root.join(entry))
            }
            PackageType::Lib => {
                let seg = import_segment(&pkg.name);
                let path = self.root.join("src").join(format!("{}.dream", seg));
                if !path.is_file() {
                    anyhow::bail!(
                        "library package '{}' expects {} (no entry field — use the conventional \
                         library root)",
                        pkg.name,
                        path.display()
                    );
                }
                Ok(path)
            }
        }
    }

    /// Deprecated name kept for call sites that mean the compile root.
    pub fn entry_path(&self) -> Result<PathBuf> {
        self.compile_root_path()
    }

    pub fn is_lib(&self) -> bool {
        self.manifest
            .package
            .as_ref()
            .is_some_and(|p| p.package_type == PackageType::Lib)
    }

    pub fn save_manifest(&self) -> Result<()> {
        self.manifest.save(&self.manifest_path())
    }

    /// Materializes every resolved package into `dream_packages/<import-segment-name>/`, and
    /// returns the lockfile that should be written to disk to pin this exact resolution.
    pub fn install(&self, resolved: &[ResolvedPackage]) -> Result<Lockfile> {
        let packages_dir = self.packages_dir();
        std::fs::create_dir_all(&packages_dir)?;

        let by_name: BTreeMap<&str, &ResolvedPackage> =
            resolved.iter().map(|p| (p.name.as_str(), p)).collect();

        let mut locked = Vec::with_capacity(resolved.len());
        for pkg in resolved {
            let source_dir = match &pkg.source {
                ResolvedSource::Path { path } => path.clone(),
                ResolvedSource::Git { checkout_dir, .. } => checkout_dir.clone(),
                ResolvedSource::Registry {
                    url,
                    tarball,
                    checksum,
                } => {
                    let entry = crate::registry::IndexEntry {
                        name: pkg.name.clone(),
                        vers: pkg.version.clone(),
                        cksum: checksum.clone(),
                        tarball: tarball.clone(),
                        ..Default::default()
                    };
                    let client = open_registry(url);
                    fetch::fetch_and_extract(client.as_ref(), &entry)?
                }
            };

            let dest = packages_dir.join(import_segment(&pkg.name));
            link_or_copy_dir(&source_dir, &dest)
                .with_context(|| format!("installing '{}' into {}", pkg.name, dest.display()))?;

            let dependencies = pkg
                .dependencies
                .iter()
                .filter_map(|dep_name| {
                    by_name
                        .get(dep_name.as_str())
                        .map(|dep| format!("{} {}", dep.name, dep.version))
                })
                .collect();

            locked.push(LockedPackage {
                name: pkg.name.clone(),
                version: pkg.version.clone(),
                source: pkg.source.lock_source(),
                checksum: match &pkg.source {
                    ResolvedSource::Registry { checksum, .. } => Some(checksum.clone()),
                    _ => None,
                },
                dependencies,
            });
        }

        if let Some(ws_root) = &self.workspace_root {
            symlink_member_packages_dirs(ws_root, &packages_dir)?;
        } else if self.manifest.is_workspace_root() {
            symlink_member_packages_dirs(&self.root, &packages_dir)?;
        }

        Ok(Lockfile::new(locked))
    }
}

/// Workspace root for install/update when `start_dir` is inside a `[workspace]` tree.
pub fn discover_install_root(start_dir: &Path) -> Result<(PathBuf, Vec<(PathBuf, Manifest)>)> {
    if let Some(ws_root) = Manifest::find_workspace_root(start_dir) {
        let ws_root = ws_root.canonicalize().unwrap_or(ws_root);
        let members = load_member_packages(&ws_root)?;
        Ok((ws_root, members))
    } else {
        let root = Manifest::find_package_root(start_dir).with_context(|| {
            format!(
                "no {} found in {} or any parent directory",
                MANIFEST_FILE_NAME,
                start_dir.display()
            )
        })?;
        let root = root.canonicalize().unwrap_or(root);
        let manifest = Manifest::load(&root.join(MANIFEST_FILE_NAME))?;
        let _ = manifest.package()?;
        Ok((root.clone(), vec![(root, manifest)]))
    }
}

fn load_member_dirs(ws_root: &Path) -> Result<Vec<PathBuf>> {
    let root_manifest = Manifest::load(&ws_root.join(MANIFEST_FILE_NAME))?;
    let ws = root_manifest
        .workspace
        .as_ref()
        .context("expected [workspace] at workspace root")?;
    let mut dirs = Vec::new();
    // Root package (if any) is part of the workspace when [package] is present.
    if root_manifest.package.is_some() {
        dirs.push(ws_root.to_path_buf());
    }
    for rel in &ws.members {
        let dir = ws_root.join(rel);
        if !dir.join(MANIFEST_FILE_NAME).is_file() {
            bail!(
                "workspace member '{}' has no {} (expected at {})",
                rel,
                MANIFEST_FILE_NAME,
                dir.join(MANIFEST_FILE_NAME).display()
            );
        }
        let canon = dir.canonicalize().with_context(|| {
            format!("resolving workspace member '{}' at {}", rel, dir.display())
        })?;
        if !dirs.iter().any(|d| same_dir(d, &canon)) {
            dirs.push(canon);
        }
    }
    Ok(dirs)
}

fn load_member_packages(ws_root: &Path) -> Result<Vec<(PathBuf, Manifest)>> {
    let mut out = Vec::new();
    for dir in load_member_dirs(ws_root)? {
        let manifest = Manifest::load(&dir.join(MANIFEST_FILE_NAME))?;
        let _ = manifest.package().with_context(|| {
            format!("workspace member {} must declare [package]", dir.display())
        })?;
        out.push((dir, manifest));
    }
    Ok(out)
}

fn member_package_names(ws_root: &Path) -> Result<Vec<String>> {
    Ok(load_member_packages(ws_root)?
        .into_iter()
        .filter_map(|(_, m)| m.package.map(|p| p.name))
        .collect())
}

fn package_is_workspace_member(ws_root: &Path, package_root: &Path) -> Result<bool> {
    let package_canon = package_root
        .canonicalize()
        .unwrap_or_else(|_| package_root.to_path_buf());
    for dir in load_member_dirs(ws_root)? {
        if same_dir(&dir, &package_canon) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let ac = a.canonicalize().unwrap_or_else(|_| a.to_path_buf());
    let bc = b.canonicalize().unwrap_or_else(|_| b.to_path_buf());
    ac == bc
}

/// Point each member's `dream_packages` at the shared workspace install directory.
fn symlink_member_packages_dirs(ws_root: &Path, packages_dir: &Path) -> Result<()> {
    for member_dir in load_member_dirs(ws_root)? {
        if same_dir(&member_dir, ws_root) {
            continue;
        }
        let dest = member_dir.join(PACKAGES_DIR_NAME);
        if dest.exists() || dest.is_symlink() {
            if dest.is_symlink() {
                std::fs::remove_file(&dest)?;
            } else if dest.is_dir() {
                std::fs::remove_dir_all(&dest)?;
            } else {
                std::fs::remove_file(&dest)?;
            }
        }
        let target = relative_path(&member_dir, packages_dir)?;
        try_symlink(&target, &dest)
            .with_context(|| format!("symlinking {} -> {}", dest.display(), target.display()))?;
    }
    Ok(())
}

/// Relative path from `from_dir` to `to` (both absolute or both relative).
fn relative_path(from_dir: &Path, to: &Path) -> Result<PathBuf> {
    let from = from_dir
        .canonicalize()
        .with_context(|| format!("canonicalizing {}", from_dir.display()))?;
    let to = to
        .canonicalize()
        .with_context(|| format!("canonicalizing {}", to.display()))?;
    pathdiff_from(&from, &to).with_context(|| {
        format!(
            "computing relative path from {} to {}",
            from.display(),
            to.display()
        )
    })
}

fn pathdiff_from(from_dir: &Path, to: &Path) -> Option<PathBuf> {
    let from_components: Vec<_> = from_dir.components().collect();
    let to_components: Vec<_> = to.components().collect();
    let mut i = 0;
    while i < from_components.len()
        && i < to_components.len()
        && from_components[i] == to_components[i]
    {
        i += 1;
    }
    let mut rel = PathBuf::new();
    for _ in i..from_components.len() {
        rel.push("..");
    }
    for c in &to_components[i..] {
        rel.push(c.as_os_str());
    }
    if rel.as_os_str().is_empty() {
        rel.push(".");
    }
    Some(rel)
}

/// Replaces `dest` with a fresh view of `src`'s contents: a symlink where the platform/filesystem
/// allows it (so edits to a local `path` dependency show up immediately), falling back to a
/// recursive copy (used for registry/git sources, and anywhere symlinks aren't permitted).
fn link_or_copy_dir(src: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        if dest.is_symlink() {
            std::fs::remove_file(dest)?;
        } else {
            std::fs::remove_dir_all(dest)?;
        }
    }

    if try_symlink(src, dest).is_ok() {
        return Ok(());
    }

    copy_dir_recursive(src, dest)
}

#[cfg(unix)]
fn try_symlink(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(src, dest)
}

#[cfg(windows)]
fn try_symlink(src: &Path, dest: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::os::windows::fs::symlink_dir(src, dest)
    } else {
        std::os::windows::fs::symlink_file(src, dest)
    }
}

#[cfg(not(any(unix, windows)))]
fn try_symlink(_src: &Path, _dest: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "symlinks not supported on this platform",
    ))
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dest_path = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}
