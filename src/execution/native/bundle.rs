//! Package-relative host runtime linkage for redistributable native executables.

use dream_abi::host_capability::HostCapability;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub(crate) fn stage_runtime(
    source_dir: &Path,
    output_dir: &Path,
    capabilities: &[HostCapability],
    spec: &dream_abi::target::TargetSpec,
) -> Result<PathBuf, String> {
    // Validate the whole family before copying, especially when output is a toolchain directory.
    for capability in capabilities {
        let source = source_dir.join(capability.library_name(spec));
        let destination = output_dir.join(capability.library_name(spec));
        let canonical_source = dunce::canonicalize(&source)
            .map_err(|e| format!("locating {}: {e}", source.display()))?;
        if dunce::canonicalize(&destination).is_ok_and(|p| p == canonical_source) {
            return Err("relocatable output must not overwrite the compiler's host runtime".into());
        }
    }
    for capability in capabilities {
        let names = std::iter::once(capability.library_name(spec)).chain(
            capability
                .companion_library_names(spec)
                .iter()
                .map(|name| name.to_string()),
        );
        for name in names {
            let source = source_dir.join(&name);
            std::fs::copy(&source, output_dir.join(&name))
                .map_err(|e| format!("bundling {}: {e}", source.display()))?;
        }
    }
    Ok(output_dir.to_path_buf())
}

/// Link arguments for the capability libraries; a portable `bundled` directory links each
/// library directly so its package-relative install name is kept.
pub(crate) fn link_runtime(
    source_dir: &Path,
    bundled: Option<&Path>,
    capabilities: &[HostCapability],
    spec: &dream_abi::target::TargetSpec,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    // A stale capability library must fail at link time, before any mixed-ABI callback runs.
    for capability in capabilities {
        let symbol = capability.abi_marker();
        args.push(
            if spec.is_msvc() {
                format!("-Wl,/include:{symbol}")
            } else if spec.is_apple() {
                format!("-Wl,-u,_{symbol}")
            } else {
                format!("-Wl,-u,{symbol}")
            }
            .into(),
        );
    }
    if spec.is_windows() {
        for capability in capabilities {
            args.push(source_dir.join(capability.import_library_name(spec)).into());
        }
        return args;
    }
    if let Some(directory) = bundled {
        for capability in capabilities {
            args.push(directory.join(capability.library_name(spec)).into());
        }
        if spec.is_ios() {
            // iOS app bundles are flat: the executable sits beside Frameworks/.
            args.push("-Wl,-rpath,@executable_path/Frameworks".into());
        } else if spec.is_apple() {
            args.push("-Wl,-rpath,@executable_path".into());
            args.push("-Wl,-rpath,@executable_path/../Frameworks".into());
        } else {
            args.push("-Wl,-rpath,$ORIGIN".into());
        }
    } else {
        args.push(format!("-L{}", source_dir.display()).into());
        for capability in capabilities {
            args.push(format!("-l{}", capability.link_name()).into());
        }
        args.push(format!("-Wl,-rpath,{}", source_dir.display()).into());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_selected_capability_requires_its_current_abi_marker() {
        let args = link_runtime(
            Path::new("/toolchain"),
            None,
            &[
                HostCapability::Core,
                HostCapability::Crypto,
                HostCapability::Net,
            ],
            &dream_abi::target::TargetSpec::host(),
        );
        let args: Vec<_> = args.iter().map(|arg| arg.to_string_lossy()).collect();
        for capability in [
            HostCapability::Core,
            HostCapability::Crypto,
            HostCapability::Net,
        ] {
            let symbol = capability.abi_marker();
            let required = if cfg!(windows) {
                format!("-Wl,/include:{symbol}")
            } else if cfg!(target_os = "macos") {
                format!("-Wl,-u,_{symbol}")
            } else {
                format!("-Wl,-u,{symbol}")
            };
            assert!(args.iter().any(|arg| *arg == required));
        }
        assert!(
            !args
                .iter()
                .any(|arg| arg.contains("dream_host_process_abi"))
        );
        assert!(!args.iter().any(|arg| arg.contains("abi_v1")));
    }

    #[test]
    fn core_only_staging_does_not_require_or_copy_other_libraries() {
        let source = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        std::fs::write(
            source
                .path()
                .join(HostCapability::Core.library_name(&dream_abi::target::TargetSpec::host())),
            b"runtime",
        )
        .unwrap();
        stage_runtime(
            source.path(),
            output.path(),
            &[HostCapability::Core],
            &dream_abi::target::TargetSpec::host(),
        )
        .unwrap();
        assert_eq!(std::fs::read_dir(output.path()).unwrap().count(), 1);
        let args = link_runtime(
            source.path(),
            Some(output.path()),
            &[HostCapability::Core],
            &dream_abi::target::TargetSpec::host(),
        );
        let args = format!("{args:?}");
        for capability in [HostCapability::Crypto, HostCapability::Process] {
            assert!(!args.contains(capability.link_name()));
        }
    }

    #[test]
    fn portable_link_uses_loader_relative_paths() {
        let args = link_runtime(
            Path::new("/toolchain"),
            Some(Path::new("/package")),
            &HostCapability::ALL,
            &dream_abi::target::TargetSpec::host(),
        );
        let args: Vec<_> = args.iter().map(|a| a.to_string_lossy()).collect();
        for capability in HostCapability::ALL {
            let expected = if cfg!(windows) {
                Path::new("/toolchain")
                    .join(capability.import_library_name(&dream_abi::target::TargetSpec::host()))
            } else {
                Path::new("/package")
                    .join(capability.library_name(&dream_abi::target::TargetSpec::host()))
            };
            assert!(
                args.iter()
                    .any(|a| a.as_ref() == expected.to_string_lossy())
            );
        }
        if cfg!(target_os = "macos") {
            assert!(
                args.iter()
                    .any(|a| a == "-Wl,-rpath,@executable_path/../Frameworks")
            );
        } else if cfg!(target_os = "linux") {
            assert!(args.iter().any(|a| a == "-Wl,-rpath,$ORIGIN"));
        }
        assert!(!args.iter().any(|a| a.starts_with("-Wl,-rpath,/")));
        assert!(!args.iter().any(|a| a.starts_with("-L")));
    }

    #[test]
    fn staging_cannot_modify_compiler_runtime() {
        let directory = tempfile::tempdir().unwrap();
        for capability in HostCapability::ALL {
            std::fs::write(
                directory
                    .path()
                    .join(capability.library_name(&dream_abi::target::TargetSpec::host())),
                b"compiler runtime",
            )
            .unwrap();
        }
        assert!(
            stage_runtime(
                directory.path(),
                directory.path(),
                &HostCapability::ALL,
                &dream_abi::target::TargetSpec::host()
            )
            .is_err()
        );
        for capability in HostCapability::ALL {
            assert_eq!(
                std::fs::read(
                    directory
                        .path()
                        .join(capability.library_name(&dream_abi::target::TargetSpec::host()))
                )
                .unwrap(),
                b"compiler runtime"
            );
        }
    }

    #[test]
    fn relative_output_links_each_bundled_library_directly() {
        if cfg!(windows) {
            return;
        }
        let args = link_runtime(
            Path::new("/toolchain"),
            Some(Path::new(".")),
            &HostCapability::ALL,
            &dream_abi::target::TargetSpec::host(),
        );
        for capability in HostCapability::ALL {
            assert!(args.iter().any(|arg| {
                Path::new(arg)
                    == Path::new(".")
                        .join(capability.library_name(&dream_abi::target::TargetSpec::host()))
            }));
        }
        assert!(!args.iter().any(|arg| arg == "-L."));
    }
}
