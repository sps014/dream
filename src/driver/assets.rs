//! `[package].assets`: files and directories copied beside a program's build output so relative
//! reads (`File.read_text("fixtures/config.json")`) resolve the same way in every host.

use std::fs;
use std::path::{Path, PathBuf};

/// Mirrors every asset under `project_root` into `out_dir`, preserving its relative path. A file
/// whose bytes already match is left alone, so an unchanged asset tree costs only reads.
pub fn sync(project_root: &Path, assets: &[String], out_dir: &Path) -> Result<(), String> {
    for asset in assets {
        let relative = Path::new(asset.trim());
        let source = project_root.join(relative);
        if !source.exists() {
            return Err(format!(
                "[package].assets entry '{asset}' does not exist under {}",
                project_root.display()
            ));
        }
        copy_tree(&source, &out_dir.join(relative))?;
    }
    Ok(())
}

/// The project's declared assets and its root, for a build whose entry sits inside a project.
pub fn for_entry(entry: &Path) -> Result<Option<(PathBuf, Vec<String>)>, String> {
    let Some(root) = dream_abi::project::find_project_root(entry) else {
        return Ok(None);
    };
    let manifest = super::project_manifest::ProjectManifest::load(&root)?;
    Ok((!manifest.assets.is_empty()).then_some((root, manifest.assets)))
}

fn copy_tree(source: &Path, dest: &Path) -> Result<(), String> {
    if source.is_dir() {
        fs::create_dir_all(dest).map_err(|e| format!("creating {}: {e}", dest.display()))?;
        let entries =
            fs::read_dir(source).map_err(|e| format!("reading {}: {e}", source.display()))?;
        let mut names = entries
            .map(|entry| entry.map(|e| e.file_name()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("reading {}: {e}", source.display()))?;
        names.sort();
        for name in names {
            copy_tree(&source.join(&name), &dest.join(&name))?;
        }
        return Ok(());
    }
    let bytes = fs::read(source).map_err(|e| format!("reading {}: {e}", source.display()))?;
    if fs::read(dest).is_ok_and(|existing| existing == bytes) {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    fs::write(dest, bytes).map_err(|e| format!("writing {}: {e}", dest.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_files_and_directories_and_skips_unchanged_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("fixtures/nested")).unwrap();
        fs::write(root.join("fixtures/nested/a.txt"), "a").unwrap();
        fs::write(root.join("config.json"), "{}").unwrap();
        let out = root.join("target/debug");
        let assets = vec!["fixtures".to_string(), "config.json".to_string()];
        sync(root, &assets, &out).unwrap();
        assert_eq!(
            fs::read_to_string(out.join("fixtures/nested/a.txt")).unwrap(),
            "a"
        );
        assert_eq!(fs::read_to_string(out.join("config.json")).unwrap(), "{}");

        let before = fs::metadata(out.join("config.json"))
            .unwrap()
            .modified()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        sync(root, &assets, &out).unwrap();
        let after = fs::metadata(out.join("config.json"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(before, after);

        fs::write(root.join("fixtures/nested/a.txt"), "b").unwrap();
        sync(root, &assets, &out).unwrap();
        assert_eq!(
            fs::read_to_string(out.join("fixtures/nested/a.txt")).unwrap(),
            "b"
        );
        assert!(sync(root, &["missing".to_string()], &out).is_err());
    }
}
