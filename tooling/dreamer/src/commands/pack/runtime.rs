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
    let names = |capability: dream_abi::host_capability::HostCapability| {
        std::iter::once(capability.library_name(spec)).chain(
            capability
                .companion_library_names(spec)
                .iter()
                .map(|name| name.to_string()),
        )
    };
    for capability in capabilities {
        for name in names(*capability) {
            let source = binary.with_file_name(&name);
            std::fs::copy(&source, directory.join(&name)).with_context(|| {
                format!("bundling {} into {}", source.display(), directory.display())
            })?;
        }
    }
    for capability in dream_abi::host_capability::HostCapability::ALL {
        if !capabilities.contains(&capability) {
            for name in names(capability) {
                crate::package_fs::remove_entry(&directory.join(name))?;
            }
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
            r#"{"native_abi_version":3,"host_capabilities":["core"]}"#,
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
            serde_json::json!({
                "native_abi_version": 3,
                "host_capabilities": HostCapability::ALL.iter().map(|capability| capability.name()).collect::<Vec<_>>()
            }).to_string(),
        )
        .unwrap();
        assert!(
            copy(
                &binary,
                &destination,
                &dream_abi::target::TargetSpec::host()
            )
            .is_err()
        );
        let spec = dream_abi::target::TargetSpec::host();
        for capability in HostCapability::ALL {
            std::fs::write(
                binary.with_file_name(capability.library_name(&spec)),
                b"host runtime",
            )
            .unwrap();
        }
        assert!(
            copy(&binary, &destination, &spec).is_err(),
            "companion libraries such as Dawn are required too"
        );
        for capability in HostCapability::ALL {
            for companion in capability.companion_library_names(&spec) {
                std::fs::write(binary.with_file_name(companion), b"companion").unwrap();
            }
        }
        copy(&binary, &destination, &spec).unwrap();
        for companion in HostCapability::Gpu.companion_library_names(&spec) {
            assert_eq!(
                std::fs::read(destination.join(companion)).unwrap(),
                b"companion"
            );
        }
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
