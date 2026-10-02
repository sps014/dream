//! Package-relative host runtime linkage for redistributable native executables.

use super::libdream_name;
#[cfg(target_os = "macos")]
use crate::driver::wasi::run_captured;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn stage_runtime(source_dir: &Path, output_dir: &Path) -> Result<PathBuf, String> {
    let source = source_dir.join(libdream_name());
    let destination = output_dir.join(libdream_name());
    let canonical_source = source
        .canonicalize()
        .map_err(|e| format!("locating {}: {e}", source.display()))?;
    if destination
        .canonicalize()
        .is_ok_and(|p| p == canonical_source)
    {
        return Err("relocatable output must not overwrite the compiler's host runtime".into());
    }
    std::fs::copy(&source, &destination)
        .map_err(|e| format!("bundling {}: {e}", source.display()))?;
    #[cfg(target_os = "macos")]
    {
        let mut edit = Command::new("install_name_tool");
        edit.arg("-id")
            .arg(format!("@rpath/{}", libdream_name()))
            .arg(&destination);
        run_captured(&mut edit, "set bundled libdream install name")?;
        let mut sign = Command::new("codesign");
        sign.args(["--force", "--sign", "-"]).arg(&destination);
        run_captured(&mut sign, "sign bundled libdream")?;
    }
    Ok(destination)
}

pub(crate) fn link_runtime(command: &mut Command, source_dir: &Path, bundled: Option<&Path>) {
    if cfg!(windows) {
        // MSVC requires the import library, not the DLL being shipped.
        command
            .arg(source_dir.join("dream.dll.lib"))
            .arg("-loldnames");
        return;
    }
    let directory = match bundled {
        Some(path) => path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new(".")),
        None => source_dir,
    };
    command
        .arg(format!("-L{}", directory.display()))
        .arg("-ldream");
    if bundled.is_some() {
        if cfg!(target_os = "macos") {
            command.args([
                "-Wl,-rpath,@executable_path",
                "-Wl,-rpath,@executable_path/../Frameworks",
            ]);
        } else {
            command.arg("-Wl,-rpath,$ORIGIN");
        }
    } else {
        command.arg(format!("-Wl,-rpath,{}", source_dir.display()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_link_uses_loader_relative_paths() {
        let mut command = Command::new("cc");
        link_runtime(
            &mut command,
            Path::new("/toolchain"),
            Some(Path::new("/package/runtime")),
        );
        let args: Vec<_> = command.get_args().map(|a| a.to_string_lossy()).collect();
        if cfg!(target_os = "macos") {
            assert!(args.iter().any(|a| a == "-L/package"));
            assert!(args
                .iter()
                .any(|a| a == "-Wl,-rpath,@executable_path/../Frameworks"));
        } else if cfg!(target_os = "linux") {
            assert!(args.iter().any(|a| a == "-Wl,-rpath,$ORIGIN"));
        } else if cfg!(windows) {
            assert!(args.iter().any(|a| a.ends_with("dream.dll.lib")));
        }
        assert!(!args.iter().any(|a| a.starts_with("-Wl,-rpath,/")));
    }

    #[test]
    fn staging_cannot_modify_compiler_runtime() {
        let directory = tempfile::tempdir().unwrap();
        let library = directory.path().join(libdream_name());
        std::fs::write(&library, b"compiler runtime").unwrap();
        assert!(stage_runtime(directory.path(), directory.path()).is_err());
        assert_eq!(std::fs::read(library).unwrap(), b"compiler runtime");
    }

    #[test]
    fn relative_output_uses_current_directory_as_library_search_path() {
        if cfg!(windows) {
            return;
        }
        let mut command = Command::new("cc");
        link_runtime(
            &mut command,
            Path::new("/toolchain"),
            Some(Path::new(libdream_name())),
        );
        assert!(command.get_args().any(|arg| arg == "-L."));
    }
}
