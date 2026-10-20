//! Pinned external tools. One table drives `dreamer toolchain install`, compiler discovery under
//! `~/.dream/toolchains`, `DREAM_<TOOL>` overrides and generator `@uses_tool` resolution, so
//! adding a tool is adding an entry.

/// Textual IR is only guaranteed to parse with the LLVM major it was written for. Must match
/// `scripts/build-llvm-dist.sh` and `scripts/fetch-dev-llvm.sh`.
pub const LLVM_VERSION: &str = "22.1.8";

/// Binaryen release used for deterministic WASM post-processing.
pub const BINARYEN_VERSION: &str = "133";

/// Zig release used as the cross-linking C compiler driver.
pub const ZIG_VERSION: &str = "0.16.0";

/// Slang shader compiler release available to generators.
pub const SLANG_VERSION: &str = "2026.20";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveKind {
    TarXz,
    TarGz,
    Zip,
}

/// What a tool's `DREAM_<TOOL>` variable names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverrideKind {
    /// The install root, or its `bin/` directory.
    Root,
    /// The tool's primary executable; the rest of the install is not readable by generators.
    Executable,
}

#[derive(Debug)]
pub struct HostArchive {
    /// `<os>-<arch>`, as returned by [`host_key`].
    pub host: &'static str,
    pub url: &'static str,
    /// The digest upstream publishes for the archive, verified before extraction.
    pub sha256: &'static str,
    pub kind: ArchiveKind,
}

#[derive(Debug)]
pub struct PinnedTool {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub version: &'static str,
    /// Executables relative to the install root, without the Windows `.exe` suffix. The first
    /// one is the primary executable an [`OverrideKind::Executable`] variable points at.
    pub executables: &'static [&'static str],
    pub env: &'static str,
    pub override_kind: OverrideKind,
    /// Directory relative to a Dream release bundle (`lib/dream`) that ships this tool.
    pub bundled: Option<&'static str>,
    /// Default per-run limit for generator tool requests.
    pub timeout_secs: u64,
    /// Whether a bare `dreamer toolchain install` downloads it.
    pub default_install: bool,
    pub archives: &'static [HostArchive],
    /// Shown instead of downloading when no archive covers the host.
    pub install_hint: &'static str,
}

impl PinnedTool {
    /// Directory under a toolchains root: `<name>-<version>`.
    pub fn install_dir(&self) -> String {
        format!("{}-{}", self.name, self.version)
    }

    pub fn archive(&self, host: &str) -> Option<&HostArchive> {
        self.archives.iter().find(|a| a.host == host)
    }

    /// The install-relative path of `executable` (`wasm-opt` → `bin/wasm-opt`).
    pub fn executable(&self, executable: &str) -> Option<&'static str> {
        self.executables
            .iter()
            .copied()
            .find(|path| path.rsplit('/').next() == Some(executable))
    }

    pub fn primary_executable(&self) -> &'static str {
        self.executables[0]
    }
}

/// `<os>-<arch>` for the archive table, or `None` for a host without pinned downloads.
pub fn host_key(os: &str, arch: &str) -> Option<&'static str> {
    Some(match (os, arch) {
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "x86_64") => "macos-x64",
        ("macos", "aarch64") => "macos-arm64",
        ("windows", "x86_64") => "windows-x64",
        ("windows", "aarch64") => "windows-arm64",
        _ => return None,
    })
}

pub fn current_host_key() -> Option<&'static str> {
    host_key(std::env::consts::OS, std::env::consts::ARCH)
}

pub fn pinned_tool(name: &str) -> Option<&'static PinnedTool> {
    PINNED_TOOLS
        .iter()
        .find(|t| t.name == name || t.aliases.contains(&name))
}

/// The pinned tool whose install provides `executable`, with its install-relative path.
pub fn tool_providing(executable: &str) -> Option<(&'static PinnedTool, &'static str)> {
    PINNED_TOOLS
        .iter()
        .find_map(|t| t.executable(executable).map(|path| (t, path)))
}

pub static PINNED_TOOLS: &[PinnedTool] = &[
    PinnedTool {
        name: "llvm",
        aliases: &[],
        version: LLVM_VERSION,
        executables: &[
            "bin/opt",
            "bin/llc",
            "bin/clang",
            "bin/clang++",
            "bin/llvm-as",
            "bin/llvm-dis",
            "bin/llvm-link",
            "bin/llvm-ar",
            "bin/llvm-profdata",
            "bin/lld",
            "bin/wasm-ld",
        ],
        env: "DREAM_LLVM",
        override_kind: OverrideKind::Root,
        bundled: Some("llvm"),
        timeout_secs: 120,
        default_install: false,
        archives: &[],
        install_hint: "LLVM ships with Dream releases in lib/dream/llvm; for a development build \
                       run scripts/fetch-dev-llvm.sh, or set DREAM_LLVM to an LLVM 22 bin/",
    },
    PinnedTool {
        name: "binaryen",
        aliases: &[],
        version: BINARYEN_VERSION,
        executables: &[
            "bin/wasm-opt",
            "bin/wasm-as",
            "bin/wasm-dis",
            "bin/wasm-merge",
            "bin/wasm-metadce",
            "bin/wasm2js",
        ],
        env: "DREAM_WASM_OPT",
        override_kind: OverrideKind::Executable,
        bundled: None,
        timeout_secs: 60,
        default_install: true,
        archives: &[
            HostArchive {
                host: "linux-x64",
                url: "https://github.com/WebAssembly/binaryen/releases/download/version_133/binaryen-version_133-x86_64-linux.tar.gz",
                sha256: "2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "linux-arm64",
                url: "https://github.com/WebAssembly/binaryen/releases/download/version_133/binaryen-version_133-aarch64-linux.tar.gz",
                sha256: "89c07ea56faf38d0fbecf36ca8ec0721756716185f265b568e133d427f299bf8",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "macos-x64",
                url: "https://github.com/WebAssembly/binaryen/releases/download/version_133/binaryen-version_133-x86_64-macos.tar.gz",
                sha256: "13a9b90be775c6389ce3d1f879cb8627bea56708ba8c122983941d53a8199b95",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "macos-arm64",
                url: "https://github.com/WebAssembly/binaryen/releases/download/version_133/binaryen-version_133-arm64-macos.tar.gz",
                sha256: "ad66da82ac13f163e424b1643f16c6dfcccc98b5966296b43e52d3cab04f84a8",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "windows-x64",
                url: "https://github.com/WebAssembly/binaryen/releases/download/version_133/binaryen-version_133-x86_64-windows.tar.gz",
                sha256: "17a2cbeac6b5693c5fbafab3838d3c65fd9c1eb38b05f5baec6c657e8c84995b",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "windows-arm64",
                url: "https://github.com/WebAssembly/binaryen/releases/download/version_133/binaryen-version_133-arm64-windows.tar.gz",
                sha256: "492a8e1847a0be1554bb9a7f384227981d60bc013aedc02d8ba1372c3943178c",
                kind: ArchiveKind::TarGz,
            },
        ],
        install_hint: "run `dreamer toolchain install binaryen` or set DREAM_WASM_OPT",
    },
    PinnedTool {
        name: "zig",
        aliases: &["cc"],
        version: ZIG_VERSION,
        executables: &["zig"],
        env: "DREAM_ZIG",
        override_kind: OverrideKind::Executable,
        bundled: None,
        timeout_secs: 120,
        default_install: true,
        archives: &[
            HostArchive {
                host: "linux-x64",
                url: "https://ziglang.org/download/0.16.0/zig-x86_64-linux-0.16.0.tar.xz",
                sha256: "70e49664a74374b48b51e6f3fdfbf437f6395d42509050588bd49abe52ba3d00",
                kind: ArchiveKind::TarXz,
            },
            HostArchive {
                host: "linux-arm64",
                url: "https://ziglang.org/download/0.16.0/zig-aarch64-linux-0.16.0.tar.xz",
                sha256: "ea4b09bfb22ec6f6c6ceac57ab63efb6b46e17ab08d21f69f3a48b38e1534f17",
                kind: ArchiveKind::TarXz,
            },
            HostArchive {
                host: "macos-x64",
                url: "https://ziglang.org/download/0.16.0/zig-x86_64-macos-0.16.0.tar.xz",
                sha256: "0387557ed1877bc6a2e1802c8391953baddba76081876301c522f52977b52ba7",
                kind: ArchiveKind::TarXz,
            },
            HostArchive {
                host: "macos-arm64",
                url: "https://ziglang.org/download/0.16.0/zig-aarch64-macos-0.16.0.tar.xz",
                sha256: "b23d70deaa879b5c2d486ed3316f7eaa53e84acf6fc9cc747de152450d401489",
                kind: ArchiveKind::TarXz,
            },
            HostArchive {
                host: "windows-x64",
                url: "https://ziglang.org/download/0.16.0/zig-x86_64-windows-0.16.0.zip",
                sha256: "68659eb5f1e4eb1437a722f1dd889c5a322c9954607f5edcf337bc3684a75a7e",
                kind: ArchiveKind::Zip,
            },
            HostArchive {
                host: "windows-arm64",
                url: "https://ziglang.org/download/0.16.0/zig-aarch64-windows-0.16.0.zip",
                sha256: "aee38316ee4111717900f45dd3130145c39289e105541d737eb8c5ed653c78ef",
                kind: ArchiveKind::Zip,
            },
        ],
        install_hint: "run `dreamer toolchain install cc` or set DREAM_ZIG",
    },
    PinnedTool {
        name: "slang",
        aliases: &[],
        version: SLANG_VERSION,
        // `dream-slang-layout` is not in the release archive: `dreamer toolchain install slang`
        // builds it into the install root (tooling/dreamer/src/toolchain/slang_layout.rs).
        executables: &["bin/slangc", "bin/dream-slang-layout"],
        env: "DREAM_SLANG",
        override_kind: OverrideKind::Root,
        bundled: None,
        timeout_secs: 120,
        default_install: false,
        archives: &[
            HostArchive {
                host: "linux-x64",
                url: "https://github.com/shader-slang/slang/releases/download/v2026.20/slang-2026.20-linux-x86_64.tar.gz",
                sha256: "e863e568c204810d6e8bc31852f056aef6763c55ea9925db2db7affa060347c9",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "linux-arm64",
                url: "https://github.com/shader-slang/slang/releases/download/v2026.20/slang-2026.20-linux-aarch64.tar.gz",
                sha256: "b0345743e2153d8c2bd33f56fb5696bcef63fc03513f054a1ae1b8d3dd0152a9",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "macos-x64",
                url: "https://github.com/shader-slang/slang/releases/download/v2026.20/slang-2026.20-macos-x86_64.tar.gz",
                sha256: "b4f01f615dc625f60c593d7d0a4bbd38e05dfa2a34bfd3973017c309d866feaa",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "macos-arm64",
                url: "https://github.com/shader-slang/slang/releases/download/v2026.20/slang-2026.20-macos-aarch64.tar.gz",
                sha256: "07fa8833fdbb343c3674c84fdd885910f495f3daaaa0c732b32980151e8b58c4",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "windows-x64",
                url: "https://github.com/shader-slang/slang/releases/download/v2026.20/slang-2026.20-windows-x86_64.tar.gz",
                sha256: "7b9bb838b105de0bb86390196ac5bcb02efc610c0542517204105cf87ec98399",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "windows-arm64",
                url: "https://github.com/shader-slang/slang/releases/download/v2026.20/slang-2026.20-windows-aarch64.tar.gz",
                sha256: "b5802a24e810b2b82e18cc796166692b4431431b66a65b108d111b668a920e7c",
                kind: ArchiveKind::TarGz,
            },
        ],
        install_hint: "run `dreamer toolchain install slang` or set DREAM_SLANG",
    },
    PinnedTool {
        name: "wasi-sdk",
        aliases: &[],
        version: "34.0",
        executables: &[
            "bin/clang",
            "bin/wasm-component-ld",
            "bin/wasm32-wasip2-clang",
            "bin/wasm32-wasip3-clang",
        ],
        env: "DREAM_WASI_SDK",
        override_kind: OverrideKind::Root,
        bundled: None,
        timeout_secs: 300,
        default_install: false,
        archives: &[
            HostArchive {
                host: "linux-arm64",
                url: "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-34/wasi-sdk-34.0-arm64-linux.tar.gz",
                sha256: "f7e243dff54d60bcc576e94d6166b69f410f2500ae4a9ceef34315be10e77971",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "macos-arm64",
                url: "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-34/wasi-sdk-34.0-arm64-macos.tar.gz",
                sha256: "9c59398106b417f8f14913380fdf0097a8cc0ff4af9eb3ce0065a859e88d49e9",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "windows-arm64",
                url: "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-34/wasi-sdk-34.0-arm64-windows.tar.gz",
                sha256: "45e1c71f3e965621e7b98ebe1d37b0e4b1f77f3e8072113ffb4534e67b1a4b7c",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "linux-x64",
                url: "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-34/wasi-sdk-34.0-x86_64-linux.tar.gz",
                sha256: "b761e3a0721dbae9c09a0059e5fdb2bf917d1b4a8a7b430fb3b5aafb0984b2c4",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "macos-x64",
                url: "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-34/wasi-sdk-34.0-x86_64-macos.tar.gz",
                sha256: "87d27fa8adc68dee59bfbf2e22a6d34ef717c34d6bf1d8af2a56fc929d9ce0eb",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "windows-x64",
                url: "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-34/wasi-sdk-34.0-x86_64-windows.tar.gz",
                sha256: "cccb5c323a9b34f0349a9b09e8804a0a7632c68c3310f4b5f437ed57d7e71d8f",
                kind: ArchiveKind::TarGz,
            },
        ],
        install_hint: "run `dreamer toolchain install wasi-sdk` or set DREAM_WASI_SDK",
    },
    PinnedTool {
        name: "wasmtime",
        aliases: &[],
        version: "49.0.4",
        executables: &["wasmtime"],
        env: "DREAM_WASMTIME",
        override_kind: OverrideKind::Root,
        bundled: None,
        timeout_secs: 300,
        default_install: false,
        archives: &[
            HostArchive {
                host: "linux-arm64",
                url: "https://github.com/bytecodealliance/wasmtime/releases/download/v49.0.4/wasmtime-v49.0.4-aarch64-linux.tar.xz",
                sha256: "9c1a6298dfb0df3b1249b531375dfb9d0d3eb0f1e1959982e3bc767251eb705d",
                kind: ArchiveKind::TarXz,
            },
            HostArchive {
                host: "macos-arm64",
                url: "https://github.com/bytecodealliance/wasmtime/releases/download/v49.0.4/wasmtime-v49.0.4-aarch64-macos.tar.xz",
                sha256: "0530d864e25ab10742222f3df582656d86d4f22226f972d65406609cb889752a",
                kind: ArchiveKind::TarXz,
            },
            HostArchive {
                host: "windows-arm64",
                url: "https://github.com/bytecodealliance/wasmtime/releases/download/v49.0.4/wasmtime-v49.0.4-aarch64-windows.zip",
                sha256: "5b438869c7964f0a8488b936853e13c8b44e1a788a2d9caab8316f4aa32c265f",
                kind: ArchiveKind::Zip,
            },
            HostArchive {
                host: "linux-x64",
                url: "https://github.com/bytecodealliance/wasmtime/releases/download/v49.0.4/wasmtime-v49.0.4-x86_64-linux.tar.xz",
                sha256: "6561abd54bdf941b6519cce883e0857cc9bce94d92c65287ed838d895a27f5a9",
                kind: ArchiveKind::TarXz,
            },
            HostArchive {
                host: "macos-x64",
                url: "https://github.com/bytecodealliance/wasmtime/releases/download/v49.0.4/wasmtime-v49.0.4-x86_64-macos.tar.xz",
                sha256: "8d5b45e0ed8b754a8f0b88faf6e25c8a7e9b92592f2ff5ec5338a7b2b75f570d",
                kind: ArchiveKind::TarXz,
            },
            HostArchive {
                host: "windows-x64",
                url: "https://github.com/bytecodealliance/wasmtime/releases/download/v49.0.4/wasmtime-v49.0.4-x86_64-windows.zip",
                sha256: "00d14880dad5b490fe5f547785a9d6c02d40f24baa289daaf110f35fc0193885",
                kind: ArchiveKind::Zip,
            },
        ],
        install_hint: "run `dreamer toolchain install wasmtime` or set DREAM_WASMTIME",
    },
    PinnedTool {
        name: "wit-bindgen",
        aliases: &[],
        version: "0.63.0",
        executables: &["wit-bindgen"],
        env: "DREAM_WIT_BINDGEN",
        override_kind: OverrideKind::Root,
        bundled: None,
        timeout_secs: 300,
        default_install: false,
        archives: &[
            HostArchive {
                host: "linux-arm64",
                url: "https://github.com/bytecodealliance/wit-bindgen/releases/download/v0.63.0/wit-bindgen-0.63.0-aarch64-linux.tar.gz",
                sha256: "1f9b4fb9f3af15374e2a2d026f6a1fcd8b48f37655f59f48a32cb421e169cbe4",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "macos-arm64",
                url: "https://github.com/bytecodealliance/wit-bindgen/releases/download/v0.63.0/wit-bindgen-0.63.0-aarch64-macos.tar.gz",
                sha256: "cccbf4b17e00c11b020b389ddfcd4d3a214bc072295566263fc8b358b336815a",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "windows-arm64",
                url: "https://github.com/bytecodealliance/wit-bindgen/releases/download/v0.63.0/wit-bindgen-0.63.0-aarch64-windows.zip",
                sha256: "b13979082b8a8032b19c3fc55e3be905fd95822062c3ed69b9b5ba2866ae85d9",
                kind: ArchiveKind::Zip,
            },
            HostArchive {
                host: "linux-x64",
                url: "https://github.com/bytecodealliance/wit-bindgen/releases/download/v0.63.0/wit-bindgen-0.63.0-x86_64-linux.tar.gz",
                sha256: "bf4efe883ab70d34a4740253911dda20caf895d58a003af58369506b04684cbe",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "macos-x64",
                url: "https://github.com/bytecodealliance/wit-bindgen/releases/download/v0.63.0/wit-bindgen-0.63.0-x86_64-macos.tar.gz",
                sha256: "d566ac4d3313bf6b524da3f85e82edc165374fb688255f16ddae4bf06aa1bfc2",
                kind: ArchiveKind::TarGz,
            },
            HostArchive {
                host: "windows-x64",
                url: "https://github.com/bytecodealliance/wit-bindgen/releases/download/v0.63.0/wit-bindgen-0.63.0-x86_64-windows.zip",
                sha256: "213393e4a8b4875a3f19856e943e43067d0315141fca5743e4cfb7795bc9af14",
                kind: ArchiveKind::Zip,
            },
        ],
        install_hint: "run `dreamer toolchain install wit-bindgen` or set DREAM_WIT_BINDGEN",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_archive_is_pinned_to_its_tool_version() {
        let mut names = std::collections::BTreeSet::new();
        for tool in PINNED_TOOLS {
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
        assert_eq!(pinned_tool("cc").unwrap().name, "zig");
        assert!(tool_providing("bash").is_none());
    }
}
