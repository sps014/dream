//! Which files, assets and hosts make up the test project `dream test <path>` names.

use crate::driver::project_manifest::ProjectManifest;
use dream_abi::project::find_project_root;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestHost {
    Native,
    Node,
}

impl TestHost {
    pub fn as_str(self) -> &'static str {
        match self {
            TestHost::Native => "native",
            TestHost::Node => "node",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestProject {
    /// Directory the runner entry sits in (imports resolve from here) and that holds `target/`.
    pub root: PathBuf,
    /// Display name: the package name, else the directory or file stem.
    pub name: String,
    /// Every `.dream` file joining the runner, sorted.
    pub files: Vec<PathBuf>,
    /// `[package].assets` of the owning package, relative to `asset_root`.
    pub assets: Vec<String>,
    pub asset_root: PathBuf,
    /// `[test].targets` (native when the package lists none).
    pub hosts: Vec<TestHost>,
}

impl TestProject {
    /// A `type = "test"` package directory runs all of its files; another package runs its
    /// `tests/` directory; a plain directory or a single file is a project of its own.
    pub fn locate(path: &Path) -> Result<Self, String> {
        let path = std::path::absolute(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if path.is_file() {
            if path.extension().and_then(|e| e.to_str()) != Some("dream") {
                return Err(format!("expected a .dream file, got '{}'", path.display()));
            }
            let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
            let owner = owning_package(&dir)?;
            let root = owner.as_ref().map_or(dir, |(root, _)| root.clone());
            let name = file_stem(&path);
            return Ok(Self::new(root, name, vec![path], owner));
        }
        if !path.is_dir() {
            return Err(format!("path not found: '{}'", path.display()));
        }
        let manifest_here = path.join(dream_abi::project::MANIFEST_FILE_NAME);
        if manifest_here.is_file() {
            let manifest = ProjectManifest::load(&path)?;
            let scan = if manifest.package_type.as_deref() == Some("test") {
                path.clone()
            } else {
                let tests = path.join("tests");
                if !tests.is_dir() {
                    return Err(format!(
                        "'{}' is not a type = \"test\" package and has no tests/ directory",
                        path.display()
                    ));
                }
                tests
            };
            let name = manifest
                .package_name
                .clone()
                .unwrap_or_else(|| file_stem(&path));
            let files = collect_dream_files(&scan)?;
            return Ok(Self::new(path.clone(), name, files, Some((path, manifest))));
        }
        let owner = owning_package(&path)?;
        let files = collect_dream_files(&path)?;
        let name = file_stem(&path);
        Ok(Self::new(path, name, files, owner))
    }

    fn new(
        root: PathBuf,
        name: String,
        files: Vec<PathBuf>,
        owner: Option<(PathBuf, ProjectManifest)>,
    ) -> Self {
        let (asset_root, assets, targets) = match owner {
            Some((dir, manifest)) => {
                let targets = if manifest.package_type.as_deref() == Some("test") {
                    manifest.test_targets
                } else {
                    Vec::new()
                };
                (dir, manifest.assets, targets)
            }
            None => (root.clone(), Vec::new(), Vec::new()),
        };
        let mut hosts: Vec<TestHost> = targets
            .iter()
            .map(|t| match t.as_str() {
                "node" => TestHost::Node,
                _ => TestHost::Native,
            })
            .collect();
        if hosts.is_empty() {
            hosts.push(TestHost::Native);
        }
        Self {
            root,
            name,
            files,
            assets,
            asset_root,
            hosts,
        }
    }

    /// The `import` path of `file` from the runner entry, or why it cannot be imported.
    pub fn import_path(&self, file: &Path) -> Result<String, String> {
        let relative = file.strip_prefix(&self.root).map_err(|_| {
            format!(
                "'{}' is outside the test project {}",
                file.display(),
                self.root.display()
            )
        })?;
        let mut segments = Vec::new();
        for component in relative.with_extension("").components() {
            let segment = component.as_os_str().to_string_lossy().into_owned();
            if !is_identifier(&segment) {
                return Err(format!(
                    "'{}': test file and directory names must be identifiers (letters, digits, \
                     '_') so the runner can import them; rename '{segment}'",
                    file.display()
                ));
            }
            segments.push(segment);
        }
        Ok(segments.join("."))
    }
}

fn owning_package(dir: &Path) -> Result<Option<(PathBuf, ProjectManifest)>, String> {
    let Some(root) = find_project_root(dir) else {
        return Ok(None);
    };
    let manifest = ProjectManifest::load(&root)?;
    Ok(manifest.package_name.is_some().then_some((root, manifest)))
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "tests".into())
}

fn is_identifier(segment: &str) -> bool {
    let mut chars = segment.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn collect_dream_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    collect_into(dir, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_into(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("read {}: {e}", dir.display()))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if matches!(name.as_str(), "target" | "generated")
            || name == dream_stdlib::PACKAGES_DIR
            || name.starts_with('.')
        {
            continue;
        }
        if path.is_dir() {
            collect_into(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("dream") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn test_package_collects_every_file_and_reads_hosts_and_assets() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("suite");
        write(
            &root.join("dream.toml"),
            "[package]\nname = \"suite\"\ntype = \"test\"\nassets = [\"fixtures\"]\n\
             [test]\ntargets = [\"native\", \"node\"]\n",
        );
        write(&root.join("a_tests.dream"), "");
        write(&root.join("nested/b_tests.dream"), "");
        write(&root.join("target/test/debug/stale.dream"), "");
        write(&root.join("generated/main/derive/stale.dream"), "");
        write(&root.join("dream_packages/dep/src/dep.dream"), "");
        let project = TestProject::locate(&root).unwrap();
        assert_eq!(project.name, "suite");
        assert_eq!(project.hosts, vec![TestHost::Native, TestHost::Node]);
        assert_eq!(project.assets, vec!["fixtures"]);
        let imports: Vec<_> = project
            .files
            .iter()
            .map(|f| project.import_path(f).unwrap())
            .collect();
        assert_eq!(imports, vec!["a_tests", "nested.b_tests"]);
    }

    #[test]
    fn bin_package_runs_its_tests_directory_and_rejects_unimportable_names() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("app");
        write(
            &root.join("dream.toml"),
            "[package]\nname = \"app\"\nentry = \"src/main.dream\"\n",
        );
        write(&root.join("src/main.dream"), "");
        write(&root.join("tests/math-tests.dream"), "");
        let project = TestProject::locate(&root).unwrap();
        assert_eq!(project.hosts, vec![TestHost::Native]);
        assert_eq!(project.files.len(), 1);
        assert!(project.import_path(&project.files[0]).is_err());
    }
}
