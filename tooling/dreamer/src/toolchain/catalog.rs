//! Pinned host build-tool downloads and SHA-256 checksums.

use super::{Host, HostArch, HostOs};
use anyhow::Result;

pub const ZIG_VERSION: &str = "0.16.0";

#[derive(Clone, Copy, Debug)]
pub enum ArchiveKind {
    TarXz,
    TarGz,
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

pub fn binaryen_artifact(host: Host) -> Result<Artifact> {
    let (triple, sha256) = match (host.os, host.arch) {
        (HostOs::Linux, HostArch::X64) => (
            "x86_64-linux",
            "2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489",
        ),
        (HostOs::Linux, HostArch::Arm64) => (
            "aarch64-linux",
            "89c07ea56faf38d0fbecf36ca8ec0721756716185f265b568e133d427f299bf8",
        ),
        (HostOs::Macos, HostArch::X64) => (
            "x86_64-macos",
            "13a9b90be775c6389ce3d1f879cb8627bea56708ba8c122983941d53a8199b95",
        ),
        (HostOs::Macos, HostArch::Arm64) => (
            "arm64-macos",
            "ad66da82ac13f163e424b1643f16c6dfcccc98b5966296b43e52d3cab04f84a8",
        ),
        (HostOs::Windows, HostArch::X64) => (
            "x86_64-windows",
            "17a2cbeac6b5693c5fbafab3838d3c65fd9c1eb38b05f5baec6c657e8c84995b",
        ),
        (HostOs::Windows, HostArch::Arm64) => (
            "arm64-windows",
            "492a8e1847a0be1554bb9a7f384227981d60bc013aedc02d8ba1372c3943178c",
        ),
    };
    let filename = format!(
        "binaryen-version_{}-{triple}.tar.gz",
        super::BINARYEN_VERSION
    );
    Ok(Artifact {
        url: format!(
            "https://github.com/WebAssembly/binaryen/releases/download/version_{}/{filename}",
            super::BINARYEN_VERSION
        ),
        sha256: sha256.into(),
        filename,
        kind: ArchiveKind::TarGz,
    })
}

pub fn artifact_for(component: super::Component, host: Host) -> Result<Artifact> {
    match component {
        super::Component::Cc => zig_artifact(host),
        super::Component::Binaryen => binaryen_artifact(host),
    }
}

pub fn dest_dir(component: super::Component) -> std::path::PathBuf {
    match component {
        super::Component::Cc => super::zig_dir(),
        super::Component::Binaryen => super::binaryen_dir(),
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
    fn binaryen_artifacts_cover_every_supported_host() {
        for os in [HostOs::Linux, HostOs::Macos, HostOs::Windows] {
            for arch in [HostArch::X64, HostArch::Arm64] {
                let artifact = binaryen_artifact(Host { os, arch }).unwrap();
                assert!(
                    artifact
                        .url
                        .contains(&format!("version_{}/", super::super::BINARYEN_VERSION))
                );
                assert_eq!(artifact.sha256.len(), 64);
                assert!(artifact.sha256.chars().all(|ch| ch.is_ascii_hexdigit()));
                assert!(matches!(artifact.kind, ArchiveKind::TarGz));
            }
        }
    }

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
