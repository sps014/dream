//! Pinned Zig, wasi-sdk and LLVM download URLs + SHA-256.

use super::{Host, HostArch, HostOs};
use anyhow::{bail, Result};

pub const ZIG_VERSION: &str = "0.16.0";
pub const WASI_SDK_VERSION: &str = "33.0";
/// Must match `dream`'s `driver::llvm_tools::LLVM_VERSION`.
pub const LLVM_VERSION: &str = "22.1.8";

#[derive(Clone, Copy, Debug)]
pub enum ArchiveKind {
    TarGz,
    TarXz,
    Zip,
}

#[derive(Clone, Debug)]
pub struct Artifact {
    pub url: String,
    pub sha256: String,
    pub filename: String,
    pub kind: ArchiveKind,
}

pub fn zig_artifact(host: Host) -> Result<Artifact> {
    let (triple, kind, sha256) = match (host.os, host.arch) {
        (HostOs::Linux, HostArch::X64) => (
            "x86_64-linux",
            ArchiveKind::TarXz,
            "70e49664a74374b48b51e6f3fdfbf437f6395d42509050588bd49abe52ba3d00",
        ),
        (HostOs::Linux, HostArch::Arm64) => (
            "aarch64-linux",
            ArchiveKind::TarXz,
            "ea4b09bfb22ec6f6c6ceac57ab63efb6b46e17ab08d21f69f3a48b38e1534f17",
        ),
        (HostOs::Macos, HostArch::X64) => (
            "x86_64-macos",
            ArchiveKind::TarXz,
            "0387557ed1877bc6a2e1802c8391953baddba76081876301c522f52977b52ba7",
        ),
        (HostOs::Macos, HostArch::Arm64) => (
            "aarch64-macos",
            ArchiveKind::TarXz,
            "b23d70deaa879b5c2d486ed3316f7eaa53e84acf6fc9cc747de152450d401489",
        ),
        (HostOs::Windows, HostArch::X64) => (
            "x86_64-windows",
            ArchiveKind::Zip,
            "68659eb5f1e4eb1437a722f1dd889c5a322c9954607f5edcf337bc3684a75a7e",
        ),
        (HostOs::Windows, HostArch::Arm64) => (
            "aarch64-windows",
            ArchiveKind::Zip,
            "aee38316ee4111717900f45dd3130145c39289e105541d737eb8c5ed653c78ef",
        ),
    };
    let ext = match kind {
        ArchiveKind::Zip => "zip",
        ArchiveKind::TarXz => "tar.xz",
        ArchiveKind::TarGz => "tar.gz",
    };
    let filename = format!("zig-{triple}-{ZIG_VERSION}.{ext}");
    Ok(Artifact {
        url: format!("https://ziglang.org/download/{ZIG_VERSION}/{filename}"),
        sha256: sha256.to_string(),
        filename,
        kind,
    })
}

pub fn wasi_extract_dir_name(host: Host) -> String {
    format!("wasi-sdk-{WASI_SDK_VERSION}-{}", wasi_asset_triple(host))
}

fn wasi_asset_triple(host: Host) -> &'static str {
    match (host.os, host.arch) {
        (HostOs::Linux, HostArch::X64) => "x86_64-linux",
        (HostOs::Linux, HostArch::Arm64) => "arm64-linux",
        (HostOs::Macos, HostArch::X64) => "x86_64-macos",
        (HostOs::Macos, HostArch::Arm64) => "arm64-macos",
        (HostOs::Windows, HostArch::X64) => "x86_64-windows",
        (HostOs::Windows, HostArch::Arm64) => "arm64-windows",
    }
}

pub fn wasi_artifact(host: Host) -> Result<Artifact> {
    let triple = wasi_asset_triple(host);
    let sha256 = match (host.os, host.arch) {
        (HostOs::Linux, HostArch::X64) => {
            "0ba8b5bfaeb2adf3f29bab5841d76cf5318ab8e1642ea195f88baba1abd47bce"
        }
        (HostOs::Linux, HostArch::Arm64) => {
            "4f98ee738c7abb45c81a94d1461fc53cc569d1cd01498951c8184d841a027844"
        }
        (HostOs::Macos, HostArch::X64) => {
            "18f3f201ba9734e6a4455b0b6410690395a55e9ffa9f6f5066f66083a94b93b3"
        }
        (HostOs::Macos, HostArch::Arm64) => {
            "85c997a2665ead91673b5bb88b7d0df3fc8900df3bfa244f720d478187bbdc78"
        }
        (HostOs::Windows, HostArch::X64) => {
            "df14ca2a2127c2d6b6be07e6f5549b3af9c1b3c0112430c200a4749970c59f06"
        }
        (HostOs::Windows, HostArch::Arm64) => {
            "2f457a62da1ce1a55e2ba77c450401b3551f27f04f0a87112b74c5aa8dd9504f"
        }
    };
    let filename = format!("wasi-sdk-{WASI_SDK_VERSION}-{triple}.tar.gz");
    Ok(Artifact {
        url: format!(
            "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-33/{filename}"
        ),
        sha256: sha256.to_string(),
        filename,
        kind: ArchiveKind::TarGz,
    })
}

pub fn llvm_artifact(host: Host) -> Result<Artifact> {
    let (filename, sha256) = match (host.os, host.arch) {
        (HostOs::Linux, HostArch::X64) => (
            format!("LLVM-{LLVM_VERSION}-Linux-X64.tar.xz"),
            "df0e1ecf16caf3489a272a5eea4eec9b0d82878f6477fa309504f918a0006384",
        ),
        (HostOs::Linux, HostArch::Arm64) => (
            format!("LLVM-{LLVM_VERSION}-Linux-ARM64.tar.xz"),
            "805efad2bb91cb4967fa569e0881d10c0f69c04461cf671cccbae19f547acc34",
        ),
        (HostOs::Macos, HostArch::Arm64) => (
            format!("LLVM-{LLVM_VERSION}-macOS-ARM64.tar.xz"),
            "f260f4f7c0d430828a81ae8a3826a1d63fc0963ec2459489308cc23b1f7eab4f",
        ),
        (HostOs::Windows, HostArch::X64) => (
            format!("clang+llvm-{LLVM_VERSION}-x86_64-pc-windows-msvc.tar.xz"),
            "d96c2cc1736f4eb7fa43cb9bbdf56d93551a9ae0a9aadb9c99c3c3b2b712a234",
        ),
        (HostOs::Windows, HostArch::Arm64) => (
            format!("clang+llvm-{LLVM_VERSION}-aarch64-pc-windows-msvc.tar.xz"),
            "de718c58ebbc5f61d58c17b90457fcf42983bc2c4a4aba3e010d108713bfd7f1",
        ),
        (HostOs::Macos, HostArch::X64) => bail!(
            "LLVM {LLVM_VERSION} has no official macOS x86_64 build; install LLVM {LLVM_VERSION} \
             yourself and point DREAM_LLVM at its bin/ directory"
        ),
    };
    Ok(Artifact {
        url: format!(
            "https://github.com/llvm/llvm-project/releases/download/llvmorg-{LLVM_VERSION}/{filename}"
        ),
        sha256: sha256.to_string(),
        filename,
        kind: ArchiveKind::TarXz,
    })
}

/// The LLVM archives ship every tool and static library (~7.5 GB unpacked). The backend needs
/// only these tools plus clang's resource directory (`lib/clang/`: headers and the compiler-rt
/// profile runtime for PGO), so extraction keeps just them.
pub const LLVM_TOOLS: &[&str] = &[
    "clang", "clang-22", "opt", "llc", "llvm-link", "llvm-dis", "llvm-as", "llvm-ar",
    "llvm-profdata", "lld", "ld.lld", "ld64.lld", "lld-link", "wasm-ld",
];

pub fn llvm_keep(rel: &std::path::Path) -> bool {
    let mut parts = rel.components().map(|c| c.as_os_str().to_string_lossy());
    match (parts.next().as_deref(), parts.next()) {
        (Some("bin"), Some(tool)) => {
            let stem = tool.strip_suffix(".exe").unwrap_or(&tool);
            parts.next().is_none() && LLVM_TOOLS.contains(&stem)
        }
        (Some("lib"), Some(sub)) => sub == "clang",
        (Some("LICENSE.TXT"), None) => true,
        _ => false,
    }
}

pub fn artifact_for(component: super::Component, host: Host) -> Result<Artifact> {
    match component {
        super::Component::Cc => zig_artifact(host),
        super::Component::WasiSdk => wasi_artifact(host),
        super::Component::Llvm => llvm_artifact(host),
    }
}

pub fn dest_dir(component: super::Component, host: Host) -> Result<std::path::PathBuf> {
    match component {
        super::Component::Cc => Ok(super::zig_dir()),
        super::Component::WasiSdk => Ok(super::wasi_sdk_dir(host)),
        super::Component::Llvm => Ok(super::llvm_dir()),
    }
}

pub fn ensure_host_supported(host: Host) -> Result<()> {
    match (host.os, host.arch) {
        (HostOs::Linux | HostOs::Macos | HostOs::Windows, HostArch::X64 | HostArch::Arm64) => {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn llvm_extraction_keeps_only_backend_tools() {
        assert!(llvm_keep(Path::new("bin/opt")));
        assert!(llvm_keep(Path::new("bin/llc.exe")));
        assert!(llvm_keep(Path::new("lib/clang/22/include/stddef.h")));
        assert!(llvm_keep(Path::new("bin/llvm-profdata")));
        assert!(llvm_keep(Path::new(
            "lib/clang/22/lib/darwin/libclang_rt.profile_osx.a"
        )));
        assert!(!llvm_keep(Path::new("bin/clang-tidy")));
        assert!(!llvm_keep(Path::new("lib/libLLVMCore.a")));
        assert!(!llvm_keep(Path::new("include/llvm/IR/Module.h")));
    }

    #[test]
    fn llvm_artifacts_are_pinned_per_host() {
        let mac = Host {
            os: HostOs::Macos,
            arch: HostArch::Arm64,
        };
        let a = llvm_artifact(mac).unwrap();
        assert!(a.url.ends_with("llvmorg-22.1.8/LLVM-22.1.8-macOS-ARM64.tar.xz"));
        assert_eq!(a.sha256.len(), 64);
        assert!(llvm_artifact(Host {
            os: HostOs::Macos,
            arch: HostArch::X64,
        })
        .is_err());
    }
}
