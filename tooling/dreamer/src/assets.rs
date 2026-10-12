//! Copies a package's `[package].assets` into host output layouts (`target/node/`, pack bundles).
//! The compiler mirrors them beside its own outputs; dreamer covers the directories it creates.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Copies every asset under `root` into `dest`, keeping its relative path, and answers the
/// distinct top-level entries it wrote (in declaration order).
pub fn copy_into(root: &Path, assets: &[String], dest: &Path) -> Result<Vec<PathBuf>> {
    let mut top = Vec::new();
    for asset in assets {
        let relative = Path::new(asset.trim());
        let source = root.join(relative);
        if !source.exists() {
            bail!(
                "[package].assets entry '{asset}' does not exist under {}",
                root.display()
            );
        }
        copy_tree(&source, &dest.join(relative))?;
        if let Some(Component::Normal(first)) = relative
            .components()
            .find(|c| matches!(c, Component::Normal(_)))
        {
            let first = PathBuf::from(first);
            if !top.contains(&first) {
                top.push(first);
            }
        }
    }
    Ok(top)
}

fn copy_tree(source: &Path, dest: &Path) -> Result<()> {
    if source.is_dir() {
        fs::create_dir_all(dest).with_context(|| format!("creating {}", dest.display()))?;
        let mut names = fs::read_dir(source)
            .with_context(|| format!("reading {}", source.display()))?
            .map(|entry| entry.map(|e| e.file_name()))
            .collect::<std::io::Result<Vec<_>>>()?;
        names.sort();
        for name in names {
            copy_tree(&source.join(&name), &dest.join(&name))?;
        }
        return Ok(());
    }
    let bytes = fs::read(source).with_context(|| format!("reading {}", source.display()))?;
    if fs::read(dest).is_ok_and(|existing| existing == bytes) {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(dest, bytes).with_context(|| format!("writing {}", dest.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_nested_assets_and_reports_top_level_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("pkg");
        fs::create_dir_all(root.join("data/deep")).unwrap();
        fs::write(root.join("data/deep/a.json"), "{}").unwrap();
        fs::write(root.join("data/b.txt"), "b").unwrap();
        fs::write(root.join("logo.png"), "png").unwrap();
        let dest = tmp.path().join("out");
        let top = copy_into(
            &root,
            &["data/deep".into(), "data/b.txt".into(), "logo.png".into()],
            &dest,
        )
        .unwrap();
        assert_eq!(top, vec![PathBuf::from("data"), PathBuf::from("logo.png")]);
        assert!(dest.join("data/deep/a.json").is_file());
        assert!(dest.join("data/b.txt").is_file());
        assert!(copy_into(&root, &["nope".into()], &dest).is_err());
    }
}
