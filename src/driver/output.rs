use std::path::{Path, PathBuf};

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
