//! Project-relative spellings for snapshot locations and the `generated` output tree.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ProjectPaths {
    /// Directory holding `dream.toml`, or the entry file's directory for a single-file program.
    pub root: PathBuf,
    pub has_manifest: bool,
}

impl ProjectPaths {
    pub fn for_entry(entry_file: &str) -> Self {
        let entry = Path::new(entry_file);
        let start = entry
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let start = dunce::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
        match dream_abi::project::find_project_root(&start) {
            Some(root) => ProjectPaths {
                root: dunce::canonicalize(&root).unwrap_or(root),
                has_manifest: true,
            },
            None => ProjectPaths {
                root: start,
                has_manifest: false,
            },
        }
    }

    /// `src/models.dream` for a file under the root, `<std>/...` unchanged, otherwise the
    /// absolute path (with `/` separators).
    pub fn rel(&self, file: Option<&str>) -> String {
        let Some(file) = file else {
            return String::new();
        };
        if dream_stdlib::is_std_source(file) {
            return file.to_string();
        }
        let path = Path::new(file);
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    pub fn generated_dir(&self) -> PathBuf {
        self.root.join("generated")
    }

    pub fn build_dir(&self) -> PathBuf {
        self.root.join("target/obj/generators")
    }
}

/// The module and declaration segments are identifiers, never arbitrary output paths.
pub(super) fn generator_dir(root: &Path, id: &str) -> PathBuf {
    let (module, name) = id.rsplit_once("::").unwrap_or(("", id));
    let mut dir = root.to_path_buf();
    for segment in module.split('.').filter(|s| !s.is_empty()) {
        dir.push(segment);
    }
    dir.join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outputs_follow_the_manifest_or_loose_entry_directory() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src/nested");
        std::fs::create_dir_all(&src).unwrap();
        let entry = src.join("main.dream");
        let loose = ProjectPaths::for_entry(entry.to_str().unwrap());
        let src = dunce::canonicalize(src).unwrap();
        assert_eq!(loose.generated_dir(), src.join("generated"));
        assert_eq!(loose.build_dir(), src.join("target/obj/generators"));
        std::fs::write(
            dir.path().join("dream.toml"),
            "[package]\nname = \"test\"\n",
        )
        .unwrap();
        let project = ProjectPaths::for_entry(entry.to_str().unwrap());
        let root = dunce::canonicalize(dir.path()).unwrap();
        assert_eq!(project.generated_dir(), root.join("generated"));
        assert_eq!(project.build_dir(), root.join("target/obj/generators"));
        assert_eq!(
            generator_dir(&project.generated_dir(), "a.b::emit"),
            root.join("generated/a/b/emit")
        );
        assert_eq!(
            generator_dir(&project.generated_dir(), "::emit"),
            root.join("generated/emit")
        );
    }

    #[test]
    fn bare_entry_file_is_relative_to_the_working_directory() {
        let cwd = dunce::canonicalize(std::env::current_dir().unwrap()).unwrap();
        let paths = ProjectPaths::for_entry("main.dream");
        let file = cwd.join("main.dream");
        if paths.has_manifest {
            assert!(file.starts_with(&paths.root));
        } else {
            assert_eq!(paths.root, cwd);
            assert_eq!(paths.rel(file.to_str()), "main.dream");
        }
    }
}
