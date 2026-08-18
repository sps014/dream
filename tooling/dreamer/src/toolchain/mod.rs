//! Optional host toolchains under `~/.dream/toolchains/` (`dreamer toolchain install`).

mod catalog;
mod install;

pub use catalog::{WASI_SDK_VERSION, ZIG_VERSION};
pub use install::{install, list, uninstall};

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Component {
    Cc,
    WasiSdk,
}

impl Component {
    pub fn parse_name(name: &str) -> Result<Self> {
        match name {
            "cc" | "zig" => Ok(Self::Cc),
            "wasi-sdk" | "wasi" => Ok(Self::WasiSdk),
            other => bail!("unknown toolchain component '{other}' (expected `cc` or `wasi-sdk`)"),
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Cc => "cc",
            Self::WasiSdk => "wasi-sdk",
        }
    }

    pub fn all() -> [Self; 2] {
        [Self::Cc, Self::WasiSdk]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostOs {
    Linux,
    Macos,
    Windows,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostArch {
    X64,
    Arm64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Host {
    pub os: HostOs,
    pub arch: HostArch,
}

impl Host {
    pub fn install_sh_triple(self) -> &'static str {
        match (self.os, self.arch) {
            (HostOs::Linux, HostArch::X64) => "linux-x64",
            (HostOs::Linux, HostArch::Arm64) => "linux-arm64",
            (HostOs::Macos, HostArch::X64) => "macos-x64",
            (HostOs::Macos, HostArch::Arm64) => "macos-arm64",
            (HostOs::Windows, HostArch::X64) => "windows-x64",
            (HostOs::Windows, HostArch::Arm64) => "windows-arm64",
        }
    }
}

pub fn detect_host() -> Result<Host> {
    let os = match std::env::consts::OS {
        "linux" => HostOs::Linux,
        "macos" => HostOs::Macos,
        "windows" => HostOs::Windows,
        other => bail!("unsupported OS for toolchain install: {other}"),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => HostArch::X64,
        "aarch64" => HostArch::Arm64,
        other => bail!("unsupported arch for toolchain install: {other}"),
    };
    Ok(Host { os, arch })
}

/// `~/.dream`, or `DREAM_HOME`'s prefix when that is not a Cargo `target/` dir / `bin/`.
pub fn dream_prefix() -> PathBuf {
    if let Ok(p) = std::env::var("DREAM_PREFIX") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if let Ok(home) = std::env::var("DREAM_HOME") {
        if !home.is_empty() {
            let p = PathBuf::from(&home);
            if p.file_name().and_then(|s| s.to_str()) == Some("bin") {
                if let Some(parent) = p.parent() {
                    return parent.to_path_buf();
                }
            }
            if !is_cargo_target_dir(&p) {
                return p;
            }
        }
    }
    default_user_dream()
}

fn default_user_dream() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".dream")
}

fn is_cargo_target_dir(p: &Path) -> bool {
    match p.file_name().and_then(|s| s.to_str()) {
        Some("debug" | "release") => {
            p.parent()
                .and_then(|par| par.file_name())
                .and_then(|s| s.to_str())
                == Some("target")
        }
        _ => false,
    }
}

pub fn toolchains_dir() -> PathBuf {
    if let Ok(p) = std::env::var("DREAM_TOOLCHAINS") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    dream_prefix().join("toolchains")
}

pub fn zig_dir() -> PathBuf {
    toolchains_dir().join(format!("zig-{ZIG_VERSION}"))
}

pub fn wasi_sdk_dir(host: Host) -> PathBuf {
    toolchains_dir().join(catalog::wasi_extract_dir_name(host))
}

pub fn zig_binary() -> PathBuf {
    let dir = zig_dir();
    if cfg!(windows) {
        dir.join("zig.exe")
    } else {
        dir.join("zig")
    }
}

pub fn wasi_clang(host: Host) -> PathBuf {
    let clang = if cfg!(windows) { "clang.exe" } else { "clang" };
    wasi_sdk_dir(host).join("bin").join(clang)
}

pub fn is_installed(component: Component, host: Host) -> bool {
    match component {
        Component::Cc => zig_binary().is_file(),
        Component::WasiSdk => wasi_clang(host).is_file(),
    }
}

pub fn toolchains_env_path() -> PathBuf {
    dream_prefix().join("toolchains.env")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_component_aliases() {
        assert_eq!(Component::parse_name("cc").unwrap(), Component::Cc);
        assert_eq!(Component::parse_name("zig").unwrap(), Component::Cc);
        assert_eq!(
            Component::parse_name("wasi-sdk").unwrap(),
            Component::WasiSdk
        );
        assert!(Component::parse_name("llvm").is_err());
    }

    #[test]
    fn detect_host_matches_build() {
        let h = detect_host().unwrap();
        match std::env::consts::OS {
            "linux" => assert_eq!(h.os, HostOs::Linux),
            "macos" => assert_eq!(h.os, HostOs::Macos),
            "windows" => assert_eq!(h.os, HostOs::Windows),
            _ => {}
        }
    }

    #[test]
    fn cargo_target_dir_is_detected() {
        let p = PathBuf::from("/repo/target/release");
        assert!(is_cargo_target_dir(&p));
        assert!(!is_cargo_target_dir(Path::new("/Users/x/.dream")));
        assert!(!is_cargo_target_dir(Path::new("/Users/x/.dream/bin")));
    }
}
