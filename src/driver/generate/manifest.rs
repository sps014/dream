//! Project-root discovery and `[[generators]]` / `[package].entry` lookups over the nearest
//! `dream.toml` (parsed by [`crate::driver::project_manifest`]).

use std::path::{Path, PathBuf};

use crate::driver::project_manifest::{GeneratorEntry, ProjectManifest};
use dream_abi::project::find_project_root;

/// `[[generators]]` entries from the entry project's `dream.toml` and from every package that owns
/// a loaded source file, keyed by the canonical generator path. A dependency's generators register
/// only once the program imports that dependency, like the generator packages of std packages.
pub fn load_manifest_generators<'f>(
    entry_file: &str,
    loaded_files: impl IntoIterator<Item = &'f String>,
) -> Vec<(String, GeneratorEntry)> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut add_root = |root: PathBuf| {
        let root = dunce::canonicalize(&root).unwrap_or(root);
        if !roots.contains(&root) {
            roots.push(root);
        }
    };
    if let Some(dir) = find_project_root(Path::new(entry_file)) {
        add_root(dir);
    }
    for file in loaded_files {
        let path = Path::new(file);
        if path.is_absolute()
            && path.is_file()
            && let Some(dir) = path.parent().and_then(find_project_root)
        {
            add_root(dir);
        }
    }
    let mut out = Vec::new();
    for dir in roots {
        let Ok(manifest) = ProjectManifest::load(&dir) else {
            continue;
        };
        out.extend(manifest.generators.into_iter().map(|entry| {
            let resolved = dir.join(&entry.path);
            let path = dunce::canonicalize(&resolved).unwrap_or(resolved);
            (path.to_string_lossy().into_owned(), entry)
        }));
    }
    out
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
    let root = find_project_root(cwd)?;
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
        let gens = load_manifest_generators(entry.to_str().unwrap(), &[]);
        assert_eq!(gens.len(), 1);
        assert!(gens[0].0.ends_with("gen.dream"), "{:?}", gens);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn loaded_dependency_packages_contribute_generators() {
        let root = temp_root("dep-gens-test");
        let dep = root.join("dream_packages/derive_kit");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(dep.join("src")).unwrap();
        std::fs::write(root.join("dream.toml"), "[package]\nname = \"app\"\n").unwrap();
        std::fs::write(
            dep.join("dream.toml"),
            "[package]\nname = \"derive_kit\"\n\n[[generators]]\npath = \"gen/derive.dream\"\n",
        )
        .unwrap();
        let dep_file = dep.join("src/derive_kit.dream");
        std::fs::write(&dep_file, "").unwrap();
        let entry = root.join("src/main.dream");
        assert!(load_manifest_generators(entry.to_str().unwrap(), &[]).is_empty());
        let loaded = vec![dep_file.to_string_lossy().into_owned()];
        let gens = load_manifest_generators(entry.to_str().unwrap(), &loaded);
        assert_eq!(gens.len(), 1);
        assert!(gens[0].0.ends_with("gen/derive.dream"), "{:?}", gens);
        let _ = std::fs::remove_dir_all(&root);
    }
}
