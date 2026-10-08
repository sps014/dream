use std::path::{Path, PathBuf};

/// Protects intermediate files and the completed artifact stamp as one output transaction.
/// Different profiles and artifact requests share the same intermediate filenames.
pub fn acquire_lock(output: &Path) -> std::io::Result<std::fs::File> {
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(output.with_extension("dream-output.lock"))?;
    lock.lock()?;
    Ok(lock)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_extensions_share_a_lock_until_publication_finishes() {
        let temp = tempfile::tempdir().unwrap();
        let guard = acquire_lock(&temp.path().join("program.ll")).unwrap();
        let other = std::fs::OpenOptions::new().read(true).write(true)
            .open(temp.path().join("program.wasm").with_extension("dream-output.lock")).unwrap();
        assert!(matches!(other.try_lock(), Err(std::fs::TryLockError::WouldBlock)));
        drop(guard);
        other.try_lock().unwrap();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    Executable,
    Staticlib,
    Dylib,
    Wasm,
}

impl OutputKind {
    pub fn is_library(self) -> bool {
        matches!(self, Self::Staticlib | Self::Dylib)
    }

    pub fn artifact_path(self, ll: &Path, target: &dream_abi::target::TargetSpec) -> PathBuf {
        ll.with_extension(match self {
            Self::Executable => "bin",
            Self::Staticlib if target.is_msvc() => "lib",
            Self::Staticlib => "a",
            Self::Dylib if target.is_windows() => "dll",
            Self::Dylib if target.is_apple() => "dylib",
            Self::Dylib => "so",
            Self::Wasm => "wasm",
        })
    }
}
