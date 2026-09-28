//! Pinned Zig download URLs + SHA-256.

use super::{Host, HostArch, HostOs};
use anyhow::Result;

pub const ZIG_VERSION: &str = "0.16.0";

#[derive(Clone, Copy, Debug)]
pub enum ArchiveKind {
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
    };
    let filename = format!("zig-{triple}-{ZIG_VERSION}.{ext}");
    Ok(Artifact {
        url: format!("https://ziglang.org/download/{ZIG_VERSION}/{filename}"),
        sha256: sha256.to_string(),
        filename,
        kind,
    })
}

pub fn artifact_for(component: super::Component, host: Host) -> Result<Artifact> {
    match component {
        super::Component::Cc => zig_artifact(host),
    }
}

pub fn dest_dir(component: super::Component) -> std::path::PathBuf {
    match component {
        super::Component::Cc => super::zig_dir(),
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

    #[test]
    fn zig_artifacts_are_pinned_per_host() {
        let mac = Host {
            os: HostOs::Macos,
            arch: HostArch::Arm64,
        };
        let a = zig_artifact(mac).unwrap();
        assert!(a.url.ends_with("/0.16.0/zig-aarch64-macos-0.16.0.tar.xz"));
        assert_eq!(a.sha256.len(), 64);
    }
}
