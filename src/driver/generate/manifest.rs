//! Project-root discovery and `[[generators]]` / `[package].entry` lookups over the nearest
//! `dream.toml` (parsed by [`crate::driver::project_manifest`]).

use std::path::{Path, PathBuf};

pub use crate::driver::project_manifest::find_project_root_from;
use crate::driver::project_manifest::{GeneratorEntry, ProjectManifest};

/// Walks from `entry_file`'s directory upward looking for `dream.toml`.
pub fn find_project_root(entry_file: &str) -> Option<PathBuf> {
    let start = Path::new(entry_file)
        .parent()
        .unwrap_or_else(|| Path::new("."));
    find_project_root_from(start)
}

/// `[[generators]]` entries from the nearest `dream.toml`, keyed by the canonical generator path.
pub fn load_manifest_generators(entry_file: &str) -> Vec<(String, GeneratorEntry)> {
    let Some(dir) = find_project_root(entry_file) else {
        return Vec::new();
    };
    let Ok(manifest) = ProjectManifest::load(&dir) else {
        return Vec::new();
    };
    manifest
        .generators
        .into_iter()
        .map(|entry| {
            let resolved = dir.join(&entry.path);
            let path = resolved.canonicalize().unwrap_or(resolved);
            (path.to_string_lossy().into_owned(), entry)
        })
        .collect()
}

/// `[package].entry` from `dream.toml`, resolved against the manifest directory.
pub fn package_entry_path(project_root: &Path) -> Option<PathBuf> {
    let manifest = ProjectManifest::load(project_root).ok()?;
    let ProjectManifest {
        entry,
        library,
        package_name,
        ..
    } = manifest;
    let rel = entry.or_else(|| {
        library.map(|_| {
            format!(
                "src/{}.dream",
                crate::driver::project_manifest::import_segment(
                    package_name.as_deref().unwrap_or_default()
                )
            )
        })
    })?;
    if rel.trim().is_empty() {
        return None;
    }
    Some(project_root.join(rel))
}

/// Default compile root when the CLI is given no file: nearest `dream.toml` + `[package].entry`.
pub fn default_compile_entry(cwd: &Path) -> Option<PathBuf> {
    let root = find_project_root_from(cwd)?;
    package_entry_path(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dream-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn default_compile_entry_joins_manifest_dir() {
        let root = temp_root("entry-test");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("dream.toml"),
            "[dependencies]\nentry = \"nope.dream\"\n[package]\nname = \"t\"\nentry = \"src/main.dream\"\n",
        )
        .unwrap();
        std::fs::write(root.join("src/main.dream"), "fun main() {}\n").unwrap();
        let nested = root.join("src");
        let entry = default_compile_entry(&nested).unwrap();
        assert_eq!(entry, root.join("src/main.dream"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn generators_resolve_against_manifest_dir() {
        let root = temp_root("gens-test");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("dream.toml"),
            "[package]\nname = \"t\"\n\n[[generators]]\npath = \"gen.dream\"\n",
        )
        .unwrap();
        let entry = root.join("src/main.dream");
        let gens = load_manifest_generators(entry.to_str().unwrap());
        assert_eq!(gens.len(), 1);
        assert!(gens[0].0.ends_with("gen.dream"), "{:?}", gens);
        let _ = std::fs::remove_dir_all(&root);
    }
}
