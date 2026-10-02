use anyhow::{Context, Result};
use std::path::Path;

fn library_name() -> &'static str {
    if cfg!(windows) {
        "dream.dll"
    } else if cfg!(target_os = "macos") {
        "libdream.dylib"
    } else {
        "libdream.so"
    }
}

pub(super) fn copy(binary: &Path, directory: &Path) -> Result<()> {
    let source = binary.with_file_name(library_name());
    std::fs::create_dir_all(directory)
        .with_context(|| format!("creating {}", directory.display()))?;
    let destination = directory.join(library_name());
    std::fs::copy(&source, &destination)
        .with_context(|| format!("bundling {} into {}", source.display(), directory.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_is_required_and_copied_into_package() {
        let root = tempfile::tempdir().unwrap();
        let binary = root.path().join("program.bin");
        let destination = root.path().join("package").join("Frameworks");
        assert!(copy(&binary, &destination).is_err());
        std::fs::write(binary.with_file_name(library_name()), b"host runtime").unwrap();
        copy(&binary, &destination).unwrap();
        assert_eq!(
            std::fs::read(destination.join(library_name())).unwrap(),
            b"host runtime"
        );
    }
}
