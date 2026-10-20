//! Resolve the pinned LLVM toolchain through the shared pinned-tool table: `DREAM_LLVM` (a `bin/`
//! directory or its parent), then the minimal LLVM a release ships in `lib/dream/llvm`, then a
//! development LLVM under `~/.dream/toolchains/llvm-<version>` (`scripts/fetch-dev-llvm.sh`).

use crate::driver::toolchain::ToolchainConfig;
use dream_abi::toolchain::LLVM_VERSION;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

/// Textual IR is only guaranteed to parse with the LLVM major it was written for, so any other
/// major is rejected.
const LLVM_MAJOR: &str = "22.";

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
    let llvm = dream_abi::toolchain::pinned_tool("llvm").expect("llvm is a pinned tool");
    let tools = LlvmTools {
        bin: config.locate_tool(llvm)?.root.join("bin"),
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

/// LLVM response files keep large runtime inventories below the Windows command-line limit.
pub(super) fn input_response<'a>(
    paths: impl IntoIterator<Item = &'a std::path::PathBuf>,
) -> std::io::Result<tempfile::NamedTempFile> {
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new()?;
    for path in paths {
        let text = path.to_str().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "tool input path is not valid UTF-8",
            )
        })?;
        let text = if cfg!(windows) {
            text.replace('\\', "/")
        } else {
            text.replace('\\', "\\\\")
        };
        writeln!(file, "\"{}\"", text.replace('"', "\\\""))?;
    }
    file.flush()?;
    Ok(file)
}
