//! Project-relative spellings for snapshot locations and the `.dream/generated` output tree.

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
        let start = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
        match crate::driver::project_manifest::find_project_root_from(&start) {
            Some(root) => ProjectPaths {
                root: root.canonicalize().unwrap_or(root),
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

    /// Where generated sources are materialized: `<root>/.dream/generated` inside a project, or a
    /// per-directory folder under the generator cache for a manifest-less program (so compiling
    /// a loose file never writes next to it).
    pub fn generated_dir(&self, cache_root: &Path) -> PathBuf {
        if self.has_manifest {
            return self.root.join(".dream").join("generated");
        }
        let key = blake3::hash(self.root.to_string_lossy().as_bytes()).to_hex();
        cache_root.join("generated").join(&key.as_str()[..16])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_entry_file_is_relative_to_the_working_directory() {
        let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
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
