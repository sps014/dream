//! Global download cache shared across every Dream project on the machine, mirroring Cargo's
//! `~/.cargo/registry` layout:
//!
//! ```text
//! ~/.dream/registry/cache/<name>-<version>.tar.gz   downloaded/verified tarballs
//! ~/.dream/registry/src/<name>-<version>/           extracted package sources
//! ~/.dream/registry/git/<sanitized-url>-<rev>/       cloned git dependencies
//! ```

use crate::registry::{IndexEntry, RegistryClient};
use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::path::{Path, PathBuf};
use tar::Archive;

pub fn dream_home() -> PathBuf {
    if let Ok(custom) = std::env::var("DREAM_HOME") {
        return PathBuf::from(custom);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".dream")
}

pub fn cache_dir() -> PathBuf {
    dream_home().join("registry").join("cache")
}

pub fn src_dir() -> PathBuf {
    dream_home().join("registry").join("src")
}

pub fn git_dir() -> PathBuf {
    dream_home().join("registry").join("git")
}

/// Downloads (if not already cached) and extracts the tarball for `entry`, returning the
/// directory its contents were extracted into.
pub fn fetch_and_extract(registry: &dyn RegistryClient, entry: &IndexEntry) -> Result<PathBuf> {
    fetch_at(&dream_home(), registry, entry)
}

fn fetch_at(home: &Path, registry: &dyn RegistryClient, entry: &IndexEntry) -> Result<PathBuf> {
    let cache_dir = || home.join("registry/cache");
    let src_dir = || home.join("registry/src");
    crate::manifest::validate_package_name(&entry.name)?;
    semver::Version::parse(&entry.vers)?;
    let identity =
        serde_json::to_vec(&(registry.base_url(), &entry.name, &entry.vers, &entry.cksum))?;
    let key = crate::registry::checksum::sha256_of(&identity).replace(':', "-");
    let cache_file = cache_dir().join(format!("{key}.tar.gz"));
    let extract_dir = src_dir().join(&key);
    std::fs::create_dir_all(cache_dir())?;
    std::fs::create_dir_all(src_dir())?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(cache_dir().join(format!("{key}.lock")))?;
    lock.lock()?;
    if !cache_file.is_file()
        || crate::registry::checksum::verify_file(&cache_file, &entry.cksum).is_err()
    {
        registry.fetch_tarball(entry, &cache_file)
            .with_context(|| format!("fetching {} {}", entry.name, entry.vers))?;
    }
    crate::registry::checksum::verify_file(&cache_file, &entry.cksum)?;
    // The archive checksum authenticates this tree; a writable sidecar cannot authenticate itself.
    let verified = tempfile::tempdir_in(src_dir())?;
    let tree = verified.path().join("tree");
    extract_tarball(&cache_file, &tree)
        .with_context(|| format!("extracting {} {}", entry.name, entry.vers))?;
    validate_identity(&tree, entry)?;
    let expected = tree_digest(&tree)?;
    if std::fs::symlink_metadata(&extract_dir).is_ok_and(|m| m.is_dir())
        && tree_digest(&extract_dir).is_ok_and(|actual| actual == expected) {
        return Ok(extract_dir);
    }
    if let Ok(metadata) = std::fs::symlink_metadata(&extract_dir) {
        if metadata.is_dir() { std::fs::remove_dir_all(&extract_dir)?; }
        else { std::fs::remove_file(&extract_dir)?; }
    }
    std::fs::rename(tree, &extract_dir)?;
    Ok(extract_dir)
}

fn validate_identity(root: &Path, entry: &IndexEntry) -> Result<()> {
    let manifest = crate::manifest::Manifest::load(&root.join(crate::manifest::MANIFEST_FILE_NAME))?;
    let package = manifest.package.context("registry archive has no package manifest")?;
    if package.name != entry.name || package.version != entry.vers {
        anyhow::bail!("registry package identity does not match its index entry");
    }
    Ok(())
}

fn tree_digest(root: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    fn visit(root: &Path, dir: &Path, hash: &mut Sha256, depth: usize) -> Result<()> {
        if depth > 128 { anyhow::bail!("extracted package exceeds path depth limit"); }
        let mut entries = std::fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            let name = path.strip_prefix(root)?.to_str().context("package path is not UTF-8")?;
            hash.update((name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            if metadata.is_dir() {
                hash.update([0]);
                visit(root, &path, hash, depth + 1)?;
            } else if metadata.is_file() {
                hash.update([1]);
                hash.update(metadata.len().to_le_bytes());
                let mut reader = std::fs::File::open(path)?;
                let mut buffer = [0; 16384];
                loop {
                    let n = std::io::Read::read(&mut reader, &mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    hash.update(&buffer[..n]);
                }
            } else {
                anyhow::bail!("unsupported extracted package entry");
            }
        }
        Ok(())
    }
    let mut hash = Sha256::new();
    visit(root, root, &mut hash, 0)?;
    Ok(format!("{:x}", hash.finalize()))
}

fn extract_tarball(tarball: &Path, dest: &Path) -> Result<()> {
    let parent = dest
        .parent()
        .context("extraction destination has no parent")?;
    let temporary = tempfile::tempdir_in(parent)?;
    let file = std::fs::File::open(tarball)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    let mut expanded = 0u64;
    for (count, entry) in archive.entries()?.enumerate() {
        if count >= 10000 {
            anyhow::bail!("package archive contains too many entries");
        }
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if path.components().count() > 128 || path.is_absolute()
            || path.components().any(|c| {
                !matches!(
                    c,
                    std::path::Component::Normal(_) | std::path::Component::CurDir
                )
            })
        {
            anyhow::bail!("package archive path escapes extraction root");
        }
        let name = path.to_str().context("package archive path is not UTF-8")?;
        if name.contains('\\') || name.contains(':') {
            anyhow::bail!("package archive contains a nonportable path");
        }
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            anyhow::bail!("package archive links and special files are forbidden");
        }
        expanded = expanded
            .checked_add(entry.size())
            .context("archive size overflow")?;
        if expanded > 100 * 1024 * 1024 {
            anyhow::bail!("package archive exceeds expanded size limit");
        }
        if !entry.unpack_in(temporary.path())? {
            anyhow::bail!("package archive path escapes extraction root");
        }
    }
    crate::package_fs::remove_entry(dest)?;
    std::fs::rename(temporary.path(), dest)?;
    Ok(())
}

/// Packages `project_dir` (`dream.toml`, `src/`, `native/`, and README if present) into a `.tar.gz` at
/// `dest_tarball`, returning the raw bytes so the caller can compute a checksum before publishing.
pub fn package_project(project_dir: &Path, dest_tarball: &Path) -> Result<Vec<u8>> {
    use flate2::Compression;
    use flate2::write::GzEncoder;

    if let Some(parent) = dest_tarball.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(dest_tarball)?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(encoder);

    let manifest_path = project_dir.join(crate::manifest::MANIFEST_FILE_NAME);
    builder.append_path_with_name(&manifest_path, crate::manifest::MANIFEST_FILE_NAME)?;

    for dir in ["src", "native"] {
        let path = project_dir.join(dir);
        if path.is_dir() {
            builder.append_dir_all(dir, &path)?;
        }
    }

    if let Some(readme_name) = find_readme_name(project_dir) {
        let path = project_dir.join(&readme_name);
        builder.append_path_with_name(&path, &readme_name)?;
    }

    builder.into_inner()?.finish()?;

    std::fs::read(dest_tarball).context("re-reading packaged tarball to checksum it")
}

/// Preferred README filenames, first match wins. The returned name is stored as the registry
/// `readme` link and packed into the published tarball.
pub fn find_readme_name(project_dir: &Path) -> Option<String> {
    for name in ["README.md", "README", "Readme.md", "readme.md"] {
        if project_dir.join(name).is_file() {
            return Some(name.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn published(root: &Path, text: &str) -> (Box<dyn RegistryClient>, IndexEntry) {
        let project = root.join("project");
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(
            project.join("dream.toml"),
            "[package]\nname = 'pkg'\nversion = '1.0.0'\ntype = 'lib'\n",
        )
        .unwrap();
        std::fs::write(project.join("src/pkg.dream"), text).unwrap();
        let tarball = root.join("staging.tar.gz");
        let bytes = package_project(&project, &tarball).unwrap();
        let client = crate::registry::open_registry(&format!("file://{}", root.display()));
        let entry = IndexEntry {
            name: "pkg".into(),
            vers: "1.0.0".into(),
            cksum: crate::registry::checksum::sha256_of(&bytes),
            tarball: "dl/pkg/pkg-1.0.0.tar.gz".into(),
            ..Default::default()
        };
        client.publish(&entry, &tarball).unwrap();
        (client, entry)
    }

    #[test]
    fn registry_identity_and_extracted_integrity_are_verified() {
        let home = tempfile::tempdir().unwrap();
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let (first, entry) = published(a.path(), "first");
        let (second, other) = published(b.path(), "second");
        let path = fetch_at(home.path(), first.as_ref(), &entry).unwrap();
        let other_path = fetch_at(home.path(), second.as_ref(), &other).unwrap();
        assert_ne!(path, other_path);
        std::fs::write(path.join("src/pkg.dream"), "tampered").unwrap();
        let verified = fetch_at(home.path(), first.as_ref(), &entry).unwrap();
        assert_eq!(
            std::fs::read_to_string(verified.join("src/pkg.dream")).unwrap(),
            "first"
        );
        let mut invalid = entry;
        invalid.name = "../escape".into();
        assert!(fetch_at(home.path(), first.as_ref(), &invalid).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn extracted_root_symlinks_are_replaced_without_touching_the_target() {
        let home = tempfile::tempdir().unwrap();
        let registry = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let (client, entry) = published(registry.path(), "verified");
        let path = fetch_at(home.path(), client.as_ref(), &entry).unwrap();
        std::fs::remove_dir_all(&path).unwrap();
        std::fs::write(outside.path().join("sentinel"), "preserved").unwrap();
        std::os::unix::fs::symlink(outside.path(), &path).unwrap();
        let repaired = fetch_at(home.path(), client.as_ref(), &entry).unwrap();
        assert!(!std::fs::symlink_metadata(repaired).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(outside.path().join("sentinel")).unwrap(), "preserved");
    }

    #[test]
    fn extraction_rejects_links_and_excessive_expansion() {
        for link in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let archive_path = temp.path().join("input.tar.gz");
            let file = std::fs::File::create(&archive_path).unwrap();
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut archive = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_mode(0o644);
            if link {
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_link_name("../../escape").unwrap();
                header.set_size(0);
                header.set_cksum();
                archive
                    .append_data(&mut header, "link", std::io::empty())
                    .unwrap();
            } else {
                header.set_size(101 * 1024 * 1024);
                header.set_cksum();
                // A truncated oversized entry is rejected before reading its payload.
                archive
                    .append_data(&mut header, "large", std::io::empty())
                    .unwrap();
            }
            archive.into_inner().unwrap().finish().unwrap();
            assert!(extract_tarball(&archive_path, &temp.path().join("out")).is_err());
        }
    }
}
