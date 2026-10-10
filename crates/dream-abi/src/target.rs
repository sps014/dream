//! Target facts shared across the driver/backend boundary, independent of the build machine.

use target_lexicon::{Architecture, Environment, HOST, OperatingSystem, Triple};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetCapabilities {
    pub linear_memory: bool,
    pub native_entry: bool,
    pub native_threads: bool,
    pub c_interop: bool,
    pub js_interop: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OsVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl std::str::FromStr for OsVersion {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parts = value
            .split('.')
            .map(|part| part.parse::<u16>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| format!("invalid minimum OS version `{value}`"))?;
        match parts.as_slice() {
            [major] => Ok(Self {
                major: *major,
                minor: 0,
                patch: 0,
            }),
            [major, minor] => Ok(Self {
                major: *major,
                minor: *minor,
                patch: 0,
            }),
            [major, minor, patch] => Ok(Self {
                major: *major,
                minor: *minor,
                patch: *patch,
            }),
            _ => Err(format!("invalid minimum OS version `{value}`")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetSpec {
    pub triple: Triple,
    pub ptr_size: u32,
    pub ptr_align: u32,
    pub os: OperatingSystem,
    pub env: Environment,
    pub min_os: Option<OsVersion>,
    pub capabilities: TargetCapabilities,
}

impl TargetSpec {
    pub fn parse(triple: &str) -> Result<Self, String> {
        let normalized = triple.replace("-simulator", "-sim");
        let (normalized, ios_version) =
            if let Some((prefix, suffix)) = normalized.split_once("-ios") {
                let (version, env) = suffix.split_once('-').unwrap_or((suffix, ""));
                if !version.is_empty() {
                    let version = version.parse::<OsVersion>()?;
                    (
                        format!(
                            "{prefix}-ios{}",
                            if env.is_empty() {
                                String::new()
                            } else {
                                format!("-{env}")
                            }
                        ),
                        Some(version),
                    )
                } else {
                    (normalized, None)
                }
            } else {
                (normalized, None)
            };
        let triple = normalized.parse::<Triple>().map_err(|e| e.to_string())?;
        let spec = Self::from_triple(triple)?;
        match ios_version {
            Some(version) => spec.with_min_os(version),
            None => Ok(spec),
        }
    }

    fn from_triple(triple: Triple) -> Result<Self, String> {
        if triple.to_string().contains(['/', '\\']) {
            return Err("target triple cannot contain path separators".into());
        }
        let ptr_size = u32::from(
            triple
                .pointer_width()
                .map_err(|_| format!("unknown pointer layout for target `{triple}`"))?
                .bytes(),
        );
        if !matches!(ptr_size, 4 | 8) {
            return Err(format!("unsupported pointer width for target `{triple}`"));
        }
        let linear_memory = triple.architecture == Architecture::Wasm32;
        if triple.architecture == Architecture::Wasm64 {
            return Err("wasm64 runtime is not supported".into());
        }
        let min_os = match triple.operating_system {
            OperatingSystem::MacOSX {
                major,
                minor,
                patch,
            } => Some(OsVersion {
                major,
                minor,
                patch,
            }),
            OperatingSystem::Ios => Some(OsVersion {
                major: 13,
                minor: 0,
                patch: 0,
            }),
            OperatingSystem::Darwin => Some(OsVersion {
                major: 11,
                minor: 0,
                patch: 0,
            }),
            _ => None,
        };
        let spec = Self {
            ptr_size,
            ptr_align: ptr_size,
            os: triple.operating_system,
            env: triple.environment,
            triple,
            min_os,
            capabilities: TargetCapabilities {
                linear_memory,
                native_entry: !linear_memory,
                native_threads: !linear_memory,
                c_interop: true,
                js_interop: linear_memory,
            },
        };
        if let Some(version) = min_os {
            spec.with_min_os(version)
        } else {
            Ok(spec)
        }
    }

    pub fn host() -> Self {
        // HOST is generated from Cargo's build target, not a user-supplied triple.
        Self::from_triple(HOST).expect("compiler build target must have a supported pointer layout")
    }

    pub fn wasm32() -> Self {
        Self::parse("wasm32-unknown-wasip1").expect("built-in wasm32 target must be valid")
    }

    pub fn is_ios(&self) -> bool {
        self.os == OperatingSystem::Ios
    }

    pub fn is_windows(&self) -> bool {
        self.os == OperatingSystem::Windows
    }

    pub fn is_apple(&self) -> bool {
        matches!(
            self.os,
            OperatingSystem::Darwin | OperatingSystem::MacOSX { .. } | OperatingSystem::Ios
        )
    }

    pub fn is_msvc(&self) -> bool {
        self.is_windows() && self.env == Environment::Msvc
    }

    pub fn is_android(&self) -> bool {
        matches!(self.env, Environment::Android | Environment::Androideabi)
    }

    pub fn llvm_triple(&self) -> String {
        if self.is_ios() {
            let v = self.min_os.expect("iOS target has a deployment version");
            let triple = self.triple.to_string();
            let (prefix, suffix) = triple.split_once("-ios").expect("iOS target spelling");
            return format!(
                "{prefix}-ios{}.{}.{}{}",
                v.major,
                v.minor,
                v.patch,
                if suffix == "-sim" {
                    "-simulator"
                } else {
                    suffix
                }
            );
        }
        self.triple.to_string()
    }

    pub fn can_link_on_host(&self) -> bool {
        self.link_compatible_with(&Self::host())
    }

    fn link_compatible_with(&self, host: &Self) -> bool {
        let macos = |os| matches!(os, OperatingSystem::Darwin | OperatingSystem::MacOSX { .. });
        // Deployment versions change availability, not the host object/linker ABI.
        let same_os = self.os == host.os || (macos(self.os) && macos(host.os));
        self.capabilities.native_entry
            && host.capabilities.native_entry
            && self.triple.architecture == host.triple.architecture
            && self.triple.vendor == host.triple.vendor
            && self.env == host.env
            && self.triple.binary_format == host.triple.binary_format
            && self.ptr_size == host.ptr_size
            && self.ptr_align == host.ptr_align
            && same_os
    }

    pub fn with_min_os(mut self, version: OsVersion) -> Result<Self, String> {
        if !matches!(
            self.os,
            OperatingSystem::Darwin | OperatingSystem::MacOSX { .. } | OperatingSystem::Ios
        ) {
            return Err("minimum OS version requires a macOS or iOS target".into());
        }
        self.min_os = Some(version);
        if self.os == OperatingSystem::Ios {
            return Ok(self);
        }
        self.os = OperatingSystem::MacOSX {
            major: version.major,
            minor: version.minor,
            patch: version.patch,
        };
        self.triple.operating_system = self.os;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_layout_is_target_owned() {
        let narrow = TargetSpec::parse("i686-unknown-linux-gnu").unwrap();
        let wide = TargetSpec::parse("x86_64-pc-windows-msvc").unwrap();
        let wasm = TargetSpec::wasm32();
        assert_eq!((narrow.ptr_size, narrow.ptr_align), (4, 4));
        assert_eq!((wide.ptr_size, wide.ptr_align), (8, 8));
        assert_eq!((wasm.ptr_size, wasm.ptr_align), (4, 4));
        assert!(wasm.capabilities.linear_memory);
        assert!(!wide.capabilities.linear_memory);
        assert!(narrow.capabilities.native_entry);
        assert!(wide.capabilities.native_entry);
        assert!(!wasm.capabilities.native_entry);
        assert!(wasm.capabilities.js_interop);
        assert!(!narrow.capabilities.js_interop);
        assert!(narrow.capabilities.c_interop);
        assert!(wasm.capabilities.c_interop);
        assert_eq!(wide.env, Environment::Msvc);
    }

    #[test]
    fn deployment_version_changes_the_llvm_triple() {
        let mac = TargetSpec::parse("aarch64-apple-darwin")
            .unwrap()
            .with_min_os("13.2".parse().unwrap())
            .unwrap();
        assert_eq!(mac.triple.to_string(), "aarch64-apple-macosx13.2.0");
        assert_eq!(mac.min_os.unwrap().minor, 2);
        assert!(
            TargetSpec::parse("x86_64-unknown-linux-gnu")
                .unwrap()
                .with_min_os("13".parse().unwrap())
                .is_err()
        );
    }

    #[test]
    fn mobile_target_spellings_and_deployment_versions_are_explicit() {
        for triple in ["arm64-apple-ios", "arm64-apple-ios-simulator"] {
            let target = TargetSpec::parse(triple).unwrap();
            assert!(target.is_ios());
            assert_eq!((target.ptr_size, target.ptr_align), (8, 8));
            assert!(!target.can_link_on_host());
            let updated = target.with_min_os("15.2".parse().unwrap()).unwrap();
            assert!(updated.llvm_triple().contains("ios15.2.0"));
            assert_eq!(TargetSpec::parse(&updated.llvm_triple()).unwrap(), updated);
        }
        for triple in ["aarch64-linux-android", "x86_64-linux-android"] {
            let target = TargetSpec::parse(triple).unwrap();
            assert!(target.is_android());
            assert_eq!(target.ptr_size, 8);
            assert!(!target.can_link_on_host());
        }
        assert!(TargetSpec::parse("arm64-apple-iosgarbage").is_err());
    }

    #[test]
    fn native_link_compatibility_ignores_only_deployment_version() {
        let host = TargetSpec::parse("aarch64-apple-darwin").unwrap();
        let newer = host.clone().with_min_os("13.2".parse().unwrap()).unwrap();
        assert!(newer.link_compatible_with(&host));
        assert!(host.link_compatible_with(&newer));
        for other in [
            "x86_64-apple-darwin",
            "aarch64-unknown-linux-gnu",
            "aarch64-pc-windows-msvc",
            "wasm32-unknown-wasip1",
        ] {
            assert!(
                !TargetSpec::parse(other)
                    .unwrap()
                    .link_compatible_with(&host)
            );
        }
        let linux = TargetSpec::parse("x86_64-unknown-linux-gnu").unwrap();
        assert!(
            !TargetSpec::parse("x86_64-unknown-linux-musl")
                .unwrap()
                .link_compatible_with(&linux)
        );
        assert!(
            !TargetSpec::parse("x86_64-unknown-linux-gnux32")
                .unwrap()
                .link_compatible_with(&linux)
        );
        assert!(!TargetSpec::wasm32().can_link_on_host());
        assert!(TargetSpec::host().can_link_on_host());
    }

    #[test]
    fn malformed_targets_and_versions_are_errors() {
        assert!(TargetSpec::parse("not-a-target").is_err());
        assert!(TargetSpec::parse("unknown-unknown-unknown").is_err());
        for value in ["", "13.x", "1.2.3.4", "65536"] {
            assert!(value.parse::<OsVersion>().is_err());
        }
    }
}
