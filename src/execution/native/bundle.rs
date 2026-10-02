//! Package-relative host runtime linkage for redistributable native executables.

use dream_abi::host_capability::HostCapability;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn stage_runtime(
    source_dir: &Path,
    output_dir: &Path,
    capabilities: &[HostCapability],
) -> Result<PathBuf, String> {
    // Validate the whole family before copying, especially when output is a toolchain directory.
    for capability in capabilities {
        let source = source_dir.join(capability.library_name());
        let destination = output_dir.join(capability.library_name());
        let canonical_source = source
            .canonicalize()
            .map_err(|e| format!("locating {}: {e}", source.display()))?;
        if destination
            .canonicalize()
            .is_ok_and(|p| p == canonical_source)
        {
            return Err("relocatable output must not overwrite the compiler's host runtime".into());
        }
    }
    for capability in capabilities {
        let source = source_dir.join(capability.library_name());
        let destination = output_dir.join(capability.library_name());
        std::fs::copy(&source, &destination)
            .map_err(|e| format!("bundling {}: {e}", source.display()))?;
    }
    Ok(output_dir.to_path_buf())
}

pub(crate) fn link_runtime(
    command: &mut Command,
    source_dir: &Path,
    bundled: Option<&Path>,
    capabilities: &[HostCapability],
) {
    if cfg!(windows) {
        for capability in capabilities {
            command.arg(source_dir.join(capability.import_library_name()));
        }
        command.arg("-loldnames");
        return;
    }
    if let Some(directory) = bundled {
        // Zig turns -L directories into native rpaths; direct inputs preserve the
        // libraries' package-relative install names without leaking the build path.
        for capability in capabilities {
            command.arg(directory.join(capability.library_name()));
        }
        if cfg!(target_os = "macos") {
            command.args([
                "-Wl,-rpath,@executable_path",
                "-Wl,-rpath,@executable_path/../Frameworks",
            ]);
        } else {
            command.arg("-Wl,-rpath,$ORIGIN");
        }
    } else {
        command.arg(format!("-L{}", source_dir.display()));
        for capability in capabilities {
            command.arg(format!("-l{}", capability.link_name()));
        }
        command.arg(format!("-Wl,-rpath,{}", source_dir.display()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_only_staging_does_not_require_or_copy_other_libraries() {
        let source = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        std::fs::write(
            source.path().join(HostCapability::Core.library_name()),
            b"runtime",
        )
        .unwrap();
        stage_runtime(source.path(), output.path(), &[HostCapability::Core]).unwrap();
        assert_eq!(std::fs::read_dir(output.path()).unwrap().count(), 1);
        let mut command = Command::new("cc");
        link_runtime(
            &mut command,
            source.path(),
            Some(output.path()),
            &[HostCapability::Core],
        );
        let args = format!("{command:?}");
        for capability in [
            HostCapability::Net,
            HostCapability::Gpu,
            HostCapability::WebView,
        ] {
            assert!(!args.contains(capability.link_name()));
        }
    }

    #[test]
    fn portable_link_uses_loader_relative_paths() {
        let mut command = Command::new("cc");
        link_runtime(
            &mut command,
            Path::new("/toolchain"),
            Some(Path::new("/package")),
            &HostCapability::ALL,
        );
        let args: Vec<_> = command.get_args().map(|a| a.to_string_lossy()).collect();
        for capability in HostCapability::ALL {
            let expected = if cfg!(windows) {
                Path::new("/toolchain").join(capability.import_library_name())
            } else {
                Path::new("/package").join(capability.library_name())
            };
            assert!(args
                .iter()
                .any(|a| a.as_ref() == expected.to_string_lossy()));
        }
        if cfg!(target_os = "macos") {
            assert!(args
                .iter()
                .any(|a| a == "-Wl,-rpath,@executable_path/../Frameworks"));
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
                directory.path().join(capability.library_name()),
                b"compiler runtime",
            )
            .unwrap();
        }
        assert!(stage_runtime(directory.path(), directory.path(), &HostCapability::ALL).is_err());
        for capability in HostCapability::ALL {
            assert_eq!(
                std::fs::read(directory.path().join(capability.library_name())).unwrap(),
                b"compiler runtime"
            );
        }
    }

    #[test]
    fn relative_output_links_each_bundled_library_directly() {
        if cfg!(windows) {
            return;
        }
        let mut command = Command::new("cc");
        link_runtime(
            &mut command,
            Path::new("/toolchain"),
            Some(Path::new(".")),
            &HostCapability::ALL,
        );
        for capability in HostCapability::ALL {
            assert!(command
                .get_args()
                .any(|arg| arg == Path::new(".").join(capability.library_name())));
        }
        assert!(!command.get_args().any(|arg| arg == "-L."));
    }
}
