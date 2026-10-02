use anyhow::{Context, Result};
use dream_abi::host_capability::HostCapability;
use std::path::Path;

pub(super) fn copy(binary: &Path, directory: &Path) -> Result<()> {
    std::fs::create_dir_all(directory)
        .with_context(|| format!("creating {}", directory.display()))?;
    for capability in HostCapability::ALL {
        let source = binary.with_file_name(capability.library_name());
        let destination = directory.join(capability.library_name());
        std::fs::copy(&source, &destination).with_context(|| {
            format!("bundling {} into {}", source.display(), directory.display())
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_capability_is_required_and_copied_into_package() {
        let root = tempfile::tempdir().unwrap();
        let binary = root.path().join("program.bin");
        let destination = root.path().join("package").join("Frameworks");
        assert!(copy(&binary, &destination).is_err());
        for capability in HostCapability::ALL {
            std::fs::write(
                binary.with_file_name(capability.library_name()),
                b"host runtime",
            )
            .unwrap();
        }
        copy(&binary, &destination).unwrap();
        for capability in HostCapability::ALL {
            assert_eq!(
                std::fs::read(destination.join(capability.library_name())).unwrap(),
                b"host runtime"
            );
        }
    }
}
