//! The one native C/C++ driver: the pinned clang, aimed at the target's triple and sysroot. Links
//! name the pinned linker for the target's object format (mold for ELF and Mach-O, lld for COFF),
//! so neither Dream nor clang takes a compiler or linker from `PATH`.

use crate::driver::toolchain::ToolchainConfig;
use crate::execution::llvm::LlvmTools;
use crate::execution::llvm::runtime::sysroot_args;
use dream_abi::target::TargetSpec;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    C,
    Cxx,
}

/// A compile command for `spec`; the caller adds `-c`, flags and inputs.
pub fn compiler(clang: &Path, config: &ToolchainConfig, spec: &TargetSpec, lang: Lang) -> Command {
    let mut command = Command::new(clang);
    if lang == Lang::Cxx {
        command.arg("--driver-mode=g++");
    }
    command.arg(format!("--target={}", spec.llvm_triple()));
    command.args(sysroot_args(config, spec));
    command
}

#[derive(Debug, Clone)]
pub struct Linker {
    clang: PathBuf,
    linker: PathBuf,
}

impl Linker {
    pub fn resolve(tools: &LlvmTools, spec: &TargetSpec) -> Result<Self, String> {
        if !spec.is_windows() && !spec.is_apple() && spec.os.to_string() != "linux" {
            return Err(format!("native linking is unsupported for {}", spec.triple));
        }
        let linker = if spec.is_msvc() {
            tools.optional_tool("lld-link")?
        } else if spec.is_windows() {
            tools.optional_tool("ld.lld")?
        } else {
            let mold = dream_abi::toolchain::pinned_tool("mold").expect("mold is a pinned tool");
            let path = tools
                .config
                .locate_tool(mold)?
                .executable(if spec.is_apple() {
                    "bin/ld64.mold"
                } else {
                    "bin/mold"
                });
            if !path.is_file() {
                return Err(format!(
                    "{} is missing; {}",
                    path.display(),
                    mold.install_hint
                ));
            }
            path
        };
        Ok(Self {
            clang: tools.clang()?,
            linker,
        })
    }

    /// A link command for `spec`; `Lang::Cxx` adds the target's C++ standard library.
    pub fn command(&self, config: &ToolchainConfig, spec: &TargetSpec, lang: Lang) -> Command {
        let mut command = compiler(&self.clang, config, spec, lang);
        // clang's Windows toolchains take the linker flavor from `-fuse-ld` and find `lld-link` /
        // `ld.lld` in clang's own directory before `PATH`.
        if spec.is_windows() {
            command.arg("-fuse-ld=lld");
        } else {
            command.arg(format!("--ld-path={}", self.linker.display()));
        }
        command.args(reproducible_link_args(spec));
        command
    }

    /// The executables whose contents decide the link output, for build stamps.
    pub fn executables(&self) -> [&Path; 2] {
        [&self.clang, &self.linker]
    }
}

/// Link outputs depend only on their inputs: no random ELF build id and no COFF timestamp. mold
/// derives the Mach-O `LC_UUID` from the output's contents.
fn reproducible_link_args(spec: &TargetSpec) -> &'static [&'static str] {
    if spec.is_msvc() {
        &["-Wl,/Brepro"]
    } else if spec.is_windows() {
        &["-Wl,--no-insert-timestamp"]
    } else if spec.is_apple() {
        &[]
    } else {
        &["-Wl,--build-id=none"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn compiles_for_the_target_triple_in_the_requested_language() {
        let config = ToolchainConfig::default();
        let ios = TargetSpec::parse("arm64-apple-ios-simulator").unwrap();
        let cxx = args(&compiler(Path::new("clang"), &config, &ios, Lang::Cxx));
        assert_eq!(cxx[0], "--driver-mode=g++");
        assert!(cxx.contains(&format!("--target={}", ios.llvm_triple())));
        let linux = TargetSpec::parse("aarch64-unknown-linux-gnu").unwrap();
        let c = args(&compiler(Path::new("clang"), &config, &linux, Lang::C));
        assert!(c.contains(&format!("--target={}", linux.llvm_triple())));
        assert!(!c.iter().any(|arg| arg == "--driver-mode=g++"));
    }

    #[test]
    fn links_through_the_format_linker_reproducibly() {
        let config = ToolchainConfig {
            sysroot: Some(PathBuf::from("/sysroot")),
            ..ToolchainConfig::default()
        };
        let linker = Linker {
            clang: "clang".into(),
            linker: "/mold/bin/mold".into(),
        };
        let linux = TargetSpec::parse("x86_64-unknown-linux-gnu").unwrap();
        let elf = args(&linker.command(&config, &linux, Lang::C));
        assert!(elf.contains(&"--ld-path=/mold/bin/mold".to_string()));
        assert!(elf.contains(&"-Wl,--build-id=none".to_string()));
        assert!(elf.contains(&"--sysroot=/sysroot".to_string()));
        let windows = TargetSpec::parse("x86_64-pc-windows-msvc").unwrap();
        let coff = args(&linker.command(&config, &windows, Lang::Cxx));
        assert!(coff.contains(&"-fuse-ld=lld".to_string()));
        assert!(coff.contains(&"-Wl,/Brepro".to_string()));
        assert!(!coff.iter().any(|a| a.starts_with("--ld-path")));
    }
}
