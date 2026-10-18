//! Locates tools from the shared pinned table: `DREAM_<TOOL>`, the release bundle, then
//! `<toolchains>/<name>-<version>` under every toolchains root.

use super::ToolchainConfig;
use dream_abi::toolchain::{OverrideKind, PinnedTool};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LocatedTool {
    pub tool: &'static PinnedTool,
    /// Directory the table's relative executable paths resolve against.
    pub root: PathBuf,
    /// Whether `root` is the tool's own install tree. An executable override only vouches for
    /// that one file, so neither the sandbox nor the fingerprint widens to its directory.
    pub owns_root: bool,
}

impl LocatedTool {
    pub fn executable(&self, relative: &str) -> PathBuf {
        let path = if self.owns_root {
            self.root.join(relative)
        } else {
            self.root.join(relative.rsplit('/').next().unwrap_or(relative))
        };
        exe(path)
    }

    pub fn primary(&self) -> PathBuf {
        self.executable(self.tool.primary_executable())
    }
}

fn exe(path: PathBuf) -> PathBuf {
    if cfg!(windows) && path.extension().is_none() {
        path.with_extension("exe")
    } else {
        path
    }
}

fn installed(tool: &'static PinnedTool, root: PathBuf) -> Option<LocatedTool> {
    let located = LocatedTool {
        tool,
        root,
        owns_root: true,
    };
    located.primary().is_file().then_some(located)
}

fn from_override(tool: &'static PinnedTool, path: &Path) -> Option<LocatedTool> {
    match tool.override_kind {
        OverrideKind::Root => installed(tool, path.to_path_buf()).or_else(|| {
            let bin = Path::new(tool.primary_executable()).parent()?;
            let mut root = path.to_path_buf();
            for _ in bin.components() {
                root = root.parent()?.to_path_buf();
            }
            installed(tool, root).filter(|t| t.root.join(bin) == path)
        }),
        OverrideKind::Executable => {
            if !path.is_file() {
                return None;
            }
            let relative = Path::new(tool.primary_executable());
            let mut root = path.to_path_buf();
            for _ in relative.components() {
                root = root.parent()?.to_path_buf();
            }
            Some(if root.join(relative) == path {
                LocatedTool {
                    tool,
                    root,
                    owns_root: false,
                }
            } else {
                LocatedTool {
                    tool,
                    root: path.parent()?.to_path_buf(),
                    owns_root: false,
                }
            })
        }
    }
}

impl ToolchainConfig {
    pub fn tool_override(&self, tool: &PinnedTool) -> Option<&PathBuf> {
        self.tool_overrides.get(tool.name)
    }

    /// The pinned tool as the compiler would run it, or the table's install hint.
    pub fn locate_tool(&self, tool: &'static PinnedTool) -> Result<LocatedTool, String> {
        if let Some(path) = self.tool_override(tool) {
            return from_override(tool, path).ok_or_else(|| {
                format!(
                    "{} points at {}, which does not contain {} {}",
                    tool.env,
                    path.display(),
                    tool.name,
                    tool.version
                )
            });
        }
        tool.bundled
            .and_then(|dir| installed(tool, self.bundle_dir()?.join(dir)))
            .or_else(|| {
                self.toolchains
                    .iter()
                    .find_map(|root| installed(tool, root.join(tool.install_dir())))
            })
            .ok_or_else(|| {
                format!(
                    "{} {} not found; {}",
                    tool.name, tool.version, tool.install_hint
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dream_abi::toolchain::pinned_tool;

    fn executable(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }

    fn config(root: &Path) -> ToolchainConfig {
        ToolchainConfig {
            toolchains: vec![root.join("toolchains")],
            exe: None,
            ..ToolchainConfig::default()
        }
    }

    #[test]
    fn installed_tools_resolve_under_their_versioned_directory() {
        let temp = tempfile::tempdir().unwrap();
        let slang = pinned_tool("slang").unwrap();
        let mut config = config(temp.path());
        config.tool_overrides.clear();
        assert!(config.locate_tool(slang).unwrap_err().contains("not found"));
        let root = temp.path().join("toolchains").join(slang.install_dir());
        executable(&exe(root.join("bin/slangc")));
        let located = config.locate_tool(slang).unwrap();
        assert_eq!(located.root, root);
        assert!(located.owns_root);
    }

    #[test]
    fn overrides_accept_a_root_its_bin_or_a_single_executable() {
        let temp = tempfile::tempdir().unwrap();
        let llvm = pinned_tool("llvm").unwrap();
        let root = temp.path().join("llvm");
        executable(&exe(root.join("bin/opt")));
        let mut config = config(temp.path());
        config
            .tool_overrides
            .insert("llvm".into(), root.join("bin"));
        assert_eq!(config.locate_tool(llvm).unwrap().root, root);
        let binaryen = pinned_tool("binaryen").unwrap();
        let loose = exe(temp.path().join("loose/wasm-opt"));
        executable(&loose);
        config.tool_overrides.insert("binaryen".into(), loose.clone());
        let located = config.locate_tool(binaryen).unwrap();
        assert!(!located.owns_root);
        assert_eq!(located.primary(), loose);
    }
}
