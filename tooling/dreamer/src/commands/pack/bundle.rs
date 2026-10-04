use anyhow::{bail, Context, Result};
use std::path::{Component, Path, PathBuf};

pub struct BundleWriter {
    output: PathBuf,
    staging: tempfile::TempDir,
}

impl BundleWriter {
    pub fn new(output: &Path) -> Result<Self> {
        std::fs::create_dir_all(output)?;
        let output = output.canonicalize()?;
        let staging = tempfile::Builder::new()
            .prefix(".bundle-")
            .tempdir_in(&output)?;
        Ok(Self { output, staging })
    }

    pub fn root(&self) -> &Path {
        self.staging.path()
    }

    pub fn path(&self, relative: impl AsRef<Path>) -> Result<PathBuf> {
        let relative = relative.as_ref();
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            bail!(
                "bundle path must stay inside staging: {}",
                relative.display()
            );
        }
        Ok(self.root().join(relative))
    }

    pub fn write(&self, relative: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> Result<()> {
        let path = self.path(relative)?;
        std::fs::create_dir_all(path.parent().context("bundle parent")?)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    pub fn copy(&self, source: &Path, relative: impl AsRef<Path>) -> Result<PathBuf> {
        let path = self.path(relative)?;
        std::fs::create_dir_all(path.parent().context("bundle parent")?)?;
        std::fs::copy(source, &path).with_context(|| format!("copying {}", source.display()))?;
        Ok(path)
    }

    pub fn publish(&self, products: &[PathBuf]) -> Result<Vec<PathBuf>> {
        let mut unique = std::collections::BTreeSet::new();
        for product in products {
            if product.components().count() != 1 || !unique.insert(product) {
                bail!("bundle products must be distinct top-level entries");
            }
            if !self.path(product)?.exists() {
                bail!("missing staged product {}", product.display());
            }
        }
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.output.join(".bundle.lock"))?;
        lock.lock()?;
        let backup = tempfile::Builder::new()
            .prefix(".previous-")
            .tempdir_in(&self.output)?;
        let mut moved: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
        for product in products {
            let destination = self.output.join(product);
            let previous = backup.path().join(product);
            let existed = std::fs::symlink_metadata(&destination).is_ok();
            let result = (|| -> Result<()> {
                if existed {
                    std::fs::rename(&destination, &previous)?;
                }
                moved.push((destination.clone(), existed.then_some(previous)));
                std::fs::rename(self.path(product)?, &destination)?;
                Ok(())
            })();
            if let Err(error) = result {
                let rollback = (|| -> Result<()> {
                    for (destination, previous) in moved.into_iter().rev() {
                        crate::package_fs::remove_entry(&destination)?;
                        if let Some(previous) = previous {
                            std::fs::rename(previous, destination)?;
                        }
                    }
                    Ok(())
                })();
                if let Err(rollback) = rollback {
                    let recovery = backup.keep();
                    bail!("publishing bundle failed: {error}; rollback failed: {rollback}; previous products preserved in {}", recovery.display());
                }
                return Err(error.context("publishing bundle"));
            }
        }
        Ok(products.iter().map(|p| self.output.join(p)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn failed_publication_restores_products_already_replaced() {
        use std::os::windows::fs::OpenOptionsExt;
        let temp = tempfile::tempdir().unwrap();
        for name in ["first", "second"] {
            std::fs::write(temp.path().join(name), b"previous").unwrap();
        }
        let writer = BundleWriter::new(temp.path()).unwrap();
        writer.write("first", b"replacement").unwrap();
        writer.write("second", b"replacement").unwrap();
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(temp.path().join("second"))
            .unwrap();
        assert!(writer.publish(&["first".into(), "second".into()]).is_err());
        drop(locked);
        for name in ["first", "second"] {
            assert_eq!(std::fs::read(temp.path().join(name)).unwrap(), b"previous");
        }
    }

    #[test]
    fn failed_staging_preserves_existing_bundle_and_replacement_removes_stale_files() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("demo.app");
        std::fs::create_dir(&old).unwrap();
        std::fs::write(old.join("stale"), b"old").unwrap();
        let writer = BundleWriter::new(temp.path()).unwrap();
        writer.write("demo.app/current", b"new").unwrap();
        assert!(writer
            .publish(&["demo.app".into(), "missing".into()])
            .is_err());
        assert_eq!(std::fs::read(old.join("stale")).unwrap(), b"old");
        writer.publish(&["demo.app".into()]).unwrap();
        assert!(!old.join("stale").exists());
        assert_eq!(std::fs::read(old.join("current")).unwrap(), b"new");
        for path in ["../escape", "/outside", "nested/../../escape", ""] {
            assert!(writer.write(path, b"bad").is_err());
        }
    }
}
