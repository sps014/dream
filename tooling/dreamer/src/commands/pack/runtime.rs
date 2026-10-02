use anyhow::{Context, Result};
use dream_abi::host_capability::HostManifest;
use std::path::Path;

pub(super) fn copy(binary: &Path, directory: &Path) -> Result<()> {
    let abi_path = binary.with_extension("abi.json");
    let manifest = HostManifest::parse(
        &std::fs::read_to_string(&abi_path)
            .with_context(|| format!("reading {}", abi_path.display()))?,
    )?;
    std::fs::create_dir_all(directory)
        .with_context(|| format!("creating {}", directory.display()))?;
    for capability in manifest.host_capabilities {
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
    use dream_abi::host_capability::HostCapability;

    #[test]
    fn copies_only_manifest_capabilities_even_with_stale_build_libraries() {
        let root = tempfile::tempdir().unwrap();
        let binary = root.path().join("program.bin");
        let destination = root.path().join("package");
        std::fs::write(
            binary.with_extension("abi.json"),
            r#"{"host_capabilities":["core"]}"#,
        )
        .unwrap();
        for capability in HostCapability::ALL {
            std::fs::write(binary.with_file_name(capability.library_name()), b"runtime").unwrap();
        }
        copy(&binary, &destination).unwrap();
        for capability in HostCapability::ALL {
            assert_eq!(
                destination.join(capability.library_name()).is_file(),
                capability == HostCapability::Core
            );
        }
    }

    #[test]
    fn every_capability_is_required_and_copied_into_package() {
        let root = tempfile::tempdir().unwrap();
        let binary = root.path().join("program.bin");
        let destination = root.path().join("package").join("Frameworks");
        std::fs::write(
            binary.with_extension("abi.json"),
            r#"{"host_capabilities":["core","net","gpu","webview"]}"#,
        )
        .unwrap();
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
