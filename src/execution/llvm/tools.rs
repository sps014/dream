//! Resolve the pinned LLVM toolchain: `DREAM_LLVM` (a `bin/` directory or its parent), then the
//! minimal LLVM a release ships in `lib/dream/llvm`, then a development LLVM under
//! `~/.dream/toolchains/llvm-*` (`scripts/fetch-dev-llvm.sh`).

use crate::driver::toolchain::ToolchainConfig;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

/// Must match `scripts/build-llvm-dist.sh`'s CI pin and `scripts/fetch-dev-llvm.sh`. Textual IR is only guaranteed to parse with
/// the LLVM major it was written for, so any other major is rejected.
pub const LLVM_VERSION: &str = "22.1.8";
const LLVM_MAJOR: &str = "22.";

const MISSING_LLVM: &str =
    "LLVM 22 not found: a Dream release ships it in lib/dream/llvm next to the binary; \
     for a development build run scripts/fetch-dev-llvm.sh, or set DREAM_LLVM to an LLVM 22 bin/";

#[derive(Clone)]
pub struct LlvmTools {
    pub bin: PathBuf,
    pub(crate) config: Arc<ToolchainConfig>,
}

impl LlvmTools {
    pub fn tool(&self, name: &str) -> PathBuf {
        let exe = if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_string()
        };
        self.bin.join(exe)
    }

    pub fn command(&self, name: &str) -> Command {
        Command::new(self.tool(name))
    }

    /// A tool only some modes need (PGO's `llvm-profdata`, the linker's `wasm-ld`), checked when
    /// first used.
    pub fn optional_tool(&self, name: &str) -> Result<PathBuf, String> {
        let p = self.tool(name);
        if p.is_file() {
            Ok(p)
        } else {
            Err(format!(
                "LLVM toolchain at {} has no `{name}`",
                self.bin.display()
            ))
        }
    }

    /// clang builds development runtimes and package C/C++ bitcode; releases prebuild the runtime.
    pub fn clang(&self) -> Result<PathBuf, String> {
        self.optional_tool("clang").map_err(|e| {
            format!("{e}; package C/C++ and runtime source builds need clang from the pinned LLVM (scripts/fetch-dev-llvm.sh)")
        })
    }
}

pub fn resolve_llvm(config: &Arc<ToolchainConfig>) -> Result<LlvmTools, String> {
    let bin = config
        .resolved_llvm
        .get_or_init(|| resolve_uncached(config))
        .clone()?;
    Ok(LlvmTools {
        bin,
        config: config.clone(),
    })
}

fn resolve_uncached(config: &Arc<ToolchainConfig>) -> Result<PathBuf, String> {
    let bin = env_bin(config)
        .or_else(|| super::bundle::bundled_llvm_bin(config).filter(|b| has_opt(b)))
        .or_else(|| installed_bin(config))
        .ok_or_else(|| MISSING_LLVM.to_string())?;
    let tools = LlvmTools {
        bin,
        config: config.clone(),
    };
    for t in ["opt", "llc", "llvm-link", "llvm-dis"] {
        if !tools.tool(t).is_file() {
            return Err(format!(
                "LLVM toolchain at {} is missing `{t}`",
                tools.bin.display()
            ));
        }
    }
    let out = tools
        .command("opt")
        .arg("--version")
        .output()
        .map_err(|e| format!("running {}: {e}", tools.tool("opt").display()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let version = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("LLVM version "))
        .unwrap_or("")
        .trim()
        .to_string();
    if !version.starts_with(LLVM_MAJOR) {
        return Err(format!(
            "LLVM at {} is version `{version}`, the backend needs {LLVM_VERSION}",
            tools.bin.display()
        ));
    }
    Ok(tools.bin)
}

fn env_bin(config: &ToolchainConfig) -> Option<PathBuf> {
    let p = config.llvm.as_ref()?.clone();
    has_opt(&p)
        .then(|| p.clone())
        .or_else(|| has_opt(&p.join("bin")).then(|| p.join("bin")))
}

fn installed_bin(config: &ToolchainConfig) -> Option<PathBuf> {
    config.toolchains.iter().find_map(|r| newest_llvm_in(r))
}

fn newest_llvm_in(toolchains: &Path) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(toolchains)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|n| n.starts_with("llvm-"))
        })
        .collect();
    dirs.sort();
    dirs.into_iter()
        .rev()
        .map(|d| d.join("bin"))
        .find(|b| has_opt(b))
}

fn has_opt(bin: &Path) -> bool {
    let opt = if cfg!(windows) { "opt.exe" } else { "opt" };
    bin.join(opt).is_file()
}
