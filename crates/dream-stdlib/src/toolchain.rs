//! The full pinned-tool table: the compiler's own tools followed by every tool a stdlib package
//! declares on its record.

use crate::STD_PACKAGES;
use dream_abi::toolchain::{COMPILER_TOOLS, PinnedTool};

pub fn pinned_tools() -> impl Iterator<Item = &'static PinnedTool> {
    COMPILER_TOOLS
        .iter()
        .chain(STD_PACKAGES.iter().flat_map(|package| package.tools))
}

pub fn pinned_tool(name: &str) -> Option<&'static PinnedTool> {
    pinned_tools().find(|t| t.is_named(name))
}

/// The pinned tool whose install provides `executable`, with its install-relative path.
pub fn tool_providing(executable: &str) -> Option<(&'static PinnedTool, &'static str)> {
    pinned_tools().find_map(|t| t.executable(executable).map(|path| (t, path)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dream_abi::toolchain::BINARYEN_VERSION;

    #[test]
    fn every_archive_is_pinned_to_its_tool_version() {
        let mut names = std::collections::BTreeSet::new();
        for tool in pinned_tools() {
            assert!(!tool.executables.is_empty(), "{}", tool.name);
            assert!(tool.env.starts_with("DREAM_"), "{}", tool.name);
            assert!(tool.timeout_secs > 0, "{}", tool.name);
            for name in std::iter::once(&tool.name).chain(tool.aliases) {
                assert!(names.insert(*name), "duplicate tool name {name}");
            }
            assert!(tool.archives.is_empty() || !tool.install_hint.is_empty());
            for archive in tool.archives {
                assert!(archive.url.contains(tool.version), "{}", archive.url);
                assert_eq!(archive.sha256.len(), 64);
                assert!(archive.sha256.chars().all(|c| c.is_ascii_hexdigit()));
                assert_eq!(
                    tool.archives
                        .iter()
                        .filter(|a| a.host == archive.host)
                        .count(),
                    1
                );
            }
        }
    }

    #[test]
    fn executables_resolve_to_their_install_paths() {
        let (tool, path) = tool_providing("wasm-opt").unwrap();
        assert_eq!((tool.name, path), ("binaryen", "bin/wasm-opt"));
        assert_eq!(tool.install_dir(), format!("binaryen-{BINARYEN_VERSION}"));
        assert_eq!(tool_providing("slangc").unwrap().0.name, "slang");
        let (tool, path) = tool_providing("dream-slang-layout").unwrap();
        assert_eq!((tool.name, path), ("slang", "bin/dream-slang-layout"));
        assert_eq!(tool_providing("ld64.mold").unwrap().0.name, "mold");
        assert!(tool_providing("bash").is_none());
    }
}
