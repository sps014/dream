//! Resolve the system C compiler driver that links native binaries: env, `dreamer toolchain` Zig,
//! then PATH, and when none exists, install the Zig toolchain once (`DREAM_NO_AUTO_INSTALL=1`
//! opts out).

use crate::driver::toolchain::ToolchainConfig;
use dream_abi::target::TargetSpec;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
mod target;

const MISSING_CC: &str =
    "no linker driver found for native builds; run `dreamer toolchain install cc`, \
     or set CC / DREAM_CC to a clang-compatible compiler";

#[derive(Debug, Clone)]
pub enum Cc {
    Program(PathBuf),
    Zig(PathBuf),
}

impl Cc {
    pub fn path(&self) -> &Path {
        match self {
            Self::Program(p) | Self::Zig(p) => p,
        }
    }
    pub fn cc_command(
        &self,
        config: &ToolchainConfig,
        spec: &TargetSpec,
    ) -> Result<Command, String> {
        target::command(self, config, spec, false)
    }

    /// The C++ driver paired with this C driver, so C++ objects and the final link agree on one
    /// C++ standard library: `zig c++` for Zig, else `DREAM_CXX` / `CXX`, else `clang++` / `c++`.
    pub fn cxx_command(
        &self,
        config: &ToolchainConfig,
        spec: &TargetSpec,
    ) -> Result<Command, String> {
        target::command(self, config, spec, true)
    }
}

const MISSING_CXX: &str = "this package has C++ sources but no C++ compiler was found; run \
     `dreamer toolchain install cc` (Zig compiles and links C++ everywhere), or set DREAM_CXX \
     to a clang++-compatible compiler";

pub fn resolve_cc(config: &ToolchainConfig) -> Result<Cc, String> {
    config
        .resolved_cc
        .get_or_init(|| resolve_uncached(config))
        .clone()
}

pub fn resolve_existing_target_cc(
    config: &ToolchainConfig,
    spec: &TargetSpec,
) -> Result<Cc, String> {
    if let Some(p) = config.program(config.cc.as_ref()) {
        return Ok(classify_program(p));
    }
    if let Some(p) = config
        .zig
        .clone()
        .filter(|p| p.is_file())
        .or_else(|| find_toolchain_zig(config))
    {
        return Ok(Cc::Zig(p));
    }
    if spec.can_link_on_host() {
        if let Some(p) = config
            .find_on_path("cc")
            .or_else(|| config.find_on_path("clang"))
        {
            return Ok(Cc::Program(p));
        }
    }
    Err(MISSING_CC.into())
}

pub fn resolve_target_cc(config: &ToolchainConfig, spec: &TargetSpec) -> Result<Cc, String> {
    if spec.can_link_on_host() || config.cc.is_some() {
        return resolve_cc(config);
    }
    config
        .zig
        .clone()
        .filter(|p| p.is_file())
        .or_else(|| find_toolchain_zig(config))
        .map(Cc::Zig)
        .map(Ok)
        .unwrap_or_else(|| auto_install_zig(config).map(Cc::Zig))
}

fn resolve_uncached(config: &ToolchainConfig) -> Result<Cc, String> {
    if let Some(p) = config.program(config.cc.as_ref()) {
        return Ok(classify_program(p));
    }
    if let Some(p) = config.zig.clone() {
        if p.is_file() {
            return Ok(Cc::Zig(p));
        }
    }
    if let Some(zig) = find_toolchain_zig(config) {
        return Ok(Cc::Zig(zig));
    }
    if let Some(p) = config.find_on_path("cc") {
        return Ok(Cc::Program(p));
    }
    if let Some(p) = config.find_on_path("clang") {
        return Ok(Cc::Program(p));
    }
    auto_install_zig(config).map(Cc::Zig)
}

/// Runs `dreamer toolchain install cc` (the dreamer beside this binary, else on PATH) once per
/// configuration, reporting its progress on stderr.
fn auto_install_zig(config: &ToolchainConfig) -> Result<PathBuf, String> {
    if config.no_auto_install {
        return Err(MISSING_CC.into());
    }
    let name = if cfg!(windows) {
        "dreamer.exe"
    } else {
        "dreamer"
    };
    let beside = config
        .exe
        .clone()
        .map(|e| std::fs::canonicalize(&e).unwrap_or(e))
        .and_then(|e| Some(e.parent()?.join(name)))
        .filter(|p| p.is_file());
    let dreamer = beside
        .or_else(|| config.find_on_path("dreamer"))
        .ok_or_else(|| MISSING_CC.to_string())?;
    eprintln!(
        "no linker found; installing the Zig toolchain (set DREAM_NO_AUTO_INSTALL=1 to skip)"
    );
    let status = Command::new(&dreamer)
        .args(["toolchain", "install", "cc"])
        .stdout(Stdio::from(std::io::stderr()))
        .status()
        .map_err(|e| format!("running {}: {e}", dreamer.display()))?;
    if !status.success() {
        return Err(format!(
            "`dreamer toolchain install cc` failed; {MISSING_CC}"
        ));
    }
    find_toolchain_zig(config).ok_or_else(|| MISSING_CC.to_string())
}

/// A C compiler driver that links with the platform's own linker, for binaries the Zig toolchain
/// cannot link: `DREAM_CC`/`CC` unless they name zig, then `cc`/`clang` on PATH.
pub fn resolve_system_cc(config: &ToolchainConfig) -> Option<PathBuf> {
    config
        .program(config.cc.as_ref())
        .filter(|p| matches!(classify_program(p.clone()), Cc::Program(_)))
        .or_else(|| config.find_on_path("cc"))
        .or_else(|| config.find_on_path("clang"))
}

fn classify_program(p: PathBuf) -> Cc {
    let name = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name == "zig" {
        Cc::Zig(p)
    } else {
        Cc::Program(p)
    }
}

fn find_toolchain_zig(config: &ToolchainConfig) -> Option<PathBuf> {
    config.toolchains.iter().find_map(|root| zig_in_dir(root))
}

pub(crate) fn installed_zig(config: &ToolchainConfig) -> Option<PathBuf> {
    config
        .program(config.cc.as_ref())
        .filter(|path| matches!(classify_program(path.clone()), Cc::Zig(_)))
        .or_else(|| {
            config
                .zig
                .clone()
                .filter(|p| p.is_file())
                .or_else(|| find_toolchain_zig(config))
        })
}

fn zig_in_dir(toolchains: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "zig.exe" } else { "zig" };
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(toolchains) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir()
                && p.file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|n| n.starts_with("zig-"))
            {
                dirs.push(p);
            }
        }
    }
    dirs.sort();
    dirs.into_iter()
        .rev()
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_zig_stem() {
        match classify_program(PathBuf::from("/opt/zig")) {
            Cc::Zig(p) => assert_eq!(p, PathBuf::from("/opt/zig")),
            Cc::Program(_) => panic!("expected zig"),
        }
        match classify_program(PathBuf::from("/usr/bin/cc")) {
            Cc::Program(p) => assert_eq!(p, PathBuf::from("/usr/bin/cc")),
            Cc::Zig(_) => panic!("expected cc"),
        }
    }
}
