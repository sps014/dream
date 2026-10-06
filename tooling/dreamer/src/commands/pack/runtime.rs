use anyhow::{Context, Result};
use dream_abi::host_capability::HostManifest;
use std::path::Path;

pub(super) fn copy(
    binary: &Path,
    directory: &Path,
    spec: &dream_abi::target::TargetSpec,
) -> Result<()> {
    let abi_path = binary.with_extension("abi.json");
    let manifest = HostManifest::parse(
        &std::fs::read_to_string(&abi_path)
            .with_context(|| format!("reading {}", abi_path.display()))?,
    )?;
    std::fs::create_dir_all(directory)
        .with_context(|| format!("creating {}", directory.display()))?;
    if binary
        .parent()
        .is_some_and(|source| source.canonicalize().ok() == directory.canonicalize().ok())
    {
        anyhow::bail!("package must not overwrite its source runtime directory");
    }
    let capabilities = &manifest.host_capabilities;
    for capability in capabilities {
        let source = binary.with_file_name(capability.library_name(spec));
        let destination = directory.join(capability.library_name(spec));
        let canonical = source
            .canonicalize()
            .with_context(|| format!("locating {}", source.display()))?;
        if destination.canonicalize().is_ok_and(|p| p == canonical) {
            anyhow::bail!("package must not overwrite its source runtime");
        }
    }
    for capability in capabilities {
        let source = binary.with_file_name(capability.library_name(spec));
        let destination = directory.join(capability.library_name(spec));
        std::fs::copy(&source, &destination).with_context(|| {
            format!("bundling {} into {}", source.display(), directory.display())
        })?;
    }
    for capability in dream_abi::host_capability::HostCapability::ALL {
        if !capabilities.contains(&capability) {
            crate::package_fs::remove_entry(&directory.join(capability.library_name(spec)))?;
        }
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
            r#"{"native_abi_version":2,"host_capabilities":["core"]}"#,
        )
        .unwrap();
        for capability in HostCapability::ALL {
            std::fs::write(
                binary.with_file_name(
                    capability.library_name(&dream_abi::target::TargetSpec::host()),
                ),
                b"runtime",
            )
            .unwrap();
        }
        copy(
            &binary,
            &destination,
            &dream_abi::target::TargetSpec::host(),
        )
        .unwrap();
        for capability in HostCapability::ALL {
            assert_eq!(
                destination
                    .join(capability.library_name(&dream_abi::target::TargetSpec::host()))
                    .is_file(),
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
            r#"{"native_abi_version":2,"host_capabilities":["core","unicode","crypto","process","timezone"]}"#,
        )
        .unwrap();
        assert!(copy(
            &binary,
            &destination,
            &dream_abi::target::TargetSpec::host()
        )
        .is_err());
        for capability in HostCapability::ALL {
            std::fs::write(
                binary.with_file_name(
                    capability.library_name(&dream_abi::target::TargetSpec::host()),
                ),
                b"host runtime",
            )
            .unwrap();
        }
        copy(
            &binary,
            &destination,
            &dream_abi::target::TargetSpec::host(),
        )
        .unwrap();
        for capability in HostCapability::ALL {
            assert_eq!(
                std::fs::read(
                    destination
                        .join(capability.library_name(&dream_abi::target::TargetSpec::host()))
                )
                .unwrap(),
                b"host runtime"
            );
        }
    }
}
