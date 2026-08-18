//! Pinned Zig and wasi-sdk download URLs + SHA-256.

use super::{Host, HostArch, HostOs};
use anyhow::Result;

pub const ZIG_VERSION: &str = "0.16.0";
pub const WASI_SDK_VERSION: &str = "33.0";

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

pub fn artifact_for(component: super::Component, host: Host) -> Result<Artifact> {
    match component {
        super::Component::Cc => zig_artifact(host),
        super::Component::WasiSdk => wasi_artifact(host),
    }
}

pub fn dest_dir(component: super::Component, host: Host) -> Result<std::path::PathBuf> {
    match component {
        super::Component::Cc => Ok(super::zig_dir()),
        super::Component::WasiSdk => Ok(super::wasi_sdk_dir(host)),
    }
}

pub fn ensure_host_supported(host: Host) -> Result<()> {
    match (host.os, host.arch) {
        (HostOs::Linux | HostOs::Macos | HostOs::Windows, HostArch::X64 | HostArch::Arm64) => {
            Ok(())
        }
    }
}
