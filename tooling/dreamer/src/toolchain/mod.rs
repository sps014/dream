//! Pinned host toolchains under `~/.dream/toolchains/` (`dreamer toolchain install`), driven by
//! the shared `dream_abi::toolchain::PINNED_TOOLS` table. A tool without archives (LLVM, which a
//! release ships in `lib/dream/llvm`) reports its install hint instead.

mod install;
mod slang_layout;

pub use install::{install, list, uninstall};

use anyhow::{Result, bail};
use dream_abi::toolchain::{PINNED_TOOLS, PinnedTool};
use std::path::{Path, PathBuf};

pub fn parse_tool(name: &str) -> Result<&'static PinnedTool> {
    match dream_abi::toolchain::pinned_tool(name) {
        Some(tool) => Ok(tool),
        None => {
            let known: Vec<_> = PINNED_TOOLS.iter().map(|t| t.name).collect();
            bail!(
                "unknown toolchain component '{name}' (expected one of: {})",
                known.join(", ")
            )
        }
    }
}

pub fn host_key() -> Result<&'static str> {
    dream_abi::toolchain::current_host_key().ok_or_else(|| {
        anyhow::anyhow!(
            "unsupported host for toolchain install: {}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    })
}

/// `~/.dream`, or `DREAM_HOME`'s prefix when that is not a Cargo `target/` dir / `bin/`.
pub fn dream_prefix() -> PathBuf {
    if let Ok(p) = std::env::var("DREAM_PREFIX")
        && !p.is_empty()
    {
        return PathBuf::from(p);
    }
    if let Ok(home) = std::env::var("DREAM_HOME")
        && !home.is_empty()
    {
        let p = PathBuf::from(&home);
        if p.file_name().and_then(|s| s.to_str()) == Some("bin")
            && let Some(parent) = p.parent()
        {
            return parent.to_path_buf();
        }
        if !is_cargo_target_dir(&p) {
            return p;
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
    if let Ok(p) = std::env::var("DREAM_TOOLCHAINS")
        && !p.is_empty()
    {
        return PathBuf::from(p);
    }
    dream_prefix().join("toolchains")
}

pub fn tool_dir(tool: &PinnedTool) -> PathBuf {
    toolchains_dir().join(tool.install_dir())
}

pub fn tool_binary(tool: &PinnedTool) -> PathBuf {
    let path = tool_dir(tool).join(tool.primary_executable());
    if cfg!(windows) {
        path.with_extension("exe")
    } else {
        path
    }
}

/// Every executable is present, so an install missing a built tool (such as an interrupted
/// `dream-slang-layout` build) is redone.
pub fn is_installed(tool: &PinnedTool) -> bool {
    tool.executables.iter().all(|executable| {
        let path = tool_dir(tool).join(executable);
        if cfg!(windows) {
            path.with_extension("exe")
        } else {
            path
        }
        .is_file()
    })
}

/// Tools a bare `dreamer toolchain install` downloads for this host.
pub fn default_tools() -> Result<Vec<&'static PinnedTool>> {
    let host = host_key()?;
    Ok(PINNED_TOOLS
        .iter()
        .filter(|t| t.default_install && t.archive(host).is_some())
        .collect())
}

pub fn toolchains_env_path() -> PathBuf {
    dream_prefix().join("toolchains.env")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_component_aliases() {
        assert_eq!(parse_tool("cc").unwrap().name, "zig");
        assert_eq!(parse_tool("zig").unwrap().name, "zig");
        assert_eq!(parse_tool("binaryen").unwrap().name, "binaryen");
        assert_eq!(parse_tool("slang").unwrap().name, "slang");
        assert!(parse_tool("llvm").unwrap().archives.is_empty());
        assert!(parse_tool("gcc").is_err());
    }

    #[test]
    fn every_supported_host_has_a_key() {
        assert!(host_key().is_ok());
    }

    #[test]
    fn cargo_target_dir_is_detected() {
        let p = PathBuf::from("/repo/target/release");
        assert!(is_cargo_target_dir(&p));
        assert!(!is_cargo_target_dir(Path::new("/Users/x/.dream")));
        assert!(!is_cargo_target_dir(Path::new("/Users/x/.dream/bin")));
    }
}
