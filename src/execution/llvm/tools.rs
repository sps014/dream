//! Resolve the pinned LLVM toolchain: `DREAM_LLVM` (a `bin/` directory or its parent), then
//! `dreamer toolchain install llvm` under `~/.dream/toolchains/llvm-*`.

use crate::execution::native::cc::toolchains_dir;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Must match `dreamer`'s `toolchain::LLVM_VERSION`. Textual IR is only guaranteed to parse with
/// the LLVM major it was written for, so any other major is rejected.
pub const LLVM_VERSION: &str = "22.1.8";
const LLVM_MAJOR: &str = "22.";

const MISSING_LLVM: &str =
    "building Dream programs needs LLVM 22; run `dreamer toolchain install llvm`, \
     or set DREAM_LLVM to an LLVM 22 bin/ directory";

#[derive(Debug, Clone)]
pub struct LlvmTools {
    pub bin: PathBuf,
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

    /// A tool only some modes need (PGO's `llvm-profdata`), checked when first used.
    pub fn optional_tool(&self, name: &str) -> Result<PathBuf, String> {
        let p = self.tool(name);
        if p.is_file() {
            Ok(p)
        } else {
            Err(format!(
                "LLVM toolchain at {} has no `{name}`; reinstall with `dreamer toolchain install llvm`",
                self.bin.display()
            ))
        }
    }
}

pub fn resolve_llvm() -> Result<LlvmTools, String> {
    static RESOLVED: OnceLock<Result<LlvmTools, String>> = OnceLock::new();
    RESOLVED.get_or_init(resolve_uncached).clone()
}

fn resolve_uncached() -> Result<LlvmTools, String> {
    let bin = env_bin()
        .or_else(installed_bin)
        .ok_or_else(|| MISSING_LLVM.to_string())?;
    let tools = LlvmTools { bin };
    for t in ["opt", "llc", "llvm-link", "llvm-dis", "clang"] {
        if !tools.tool(t).is_file() {
            return Err(format!(
                "LLVM toolchain at {} is missing `{t}`; reinstall with `dreamer toolchain install llvm`",
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
            "LLVM at {} is version `{version}`, the backend needs {LLVM_VERSION}; \
             run `dreamer toolchain install llvm`",
            tools.bin.display()
        ));
    }
    Ok(tools)
}

fn env_bin() -> Option<PathBuf> {
    let v = std::env::var("DREAM_LLVM").ok().filter(|v| !v.is_empty())?;
    let p = PathBuf::from(v);
    has_opt(&p)
        .then(|| p.clone())
        .or_else(|| has_opt(&p.join("bin")).then(|| p.join("bin")))
}

fn installed_bin() -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(dir) = std::env::var("DREAM_TOOLCHAINS") {
        if !dir.is_empty() {
            roots.push(PathBuf::from(dir));
        }
    }
    roots.push(toolchains_dir());
    roots.iter().find_map(|r| newest_llvm_in(r))
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
