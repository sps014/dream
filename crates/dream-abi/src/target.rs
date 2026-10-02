//! Target facts shared across the driver/backend boundary, independent of the build machine.

use target_lexicon::{Architecture, Environment, OperatingSystem, Triple, HOST};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetCapabilities {
    pub linear_memory: bool,
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
        let triple = triple.parse::<Triple>().map_err(|e| e.to_string())?;
        Self::from_triple(triple)
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
                native_threads: !linear_memory,
                c_interop: !linear_memory,
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

    pub fn with_min_os(mut self, version: OsVersion) -> Result<Self, String> {
        if !matches!(
            self.os,
            OperatingSystem::Darwin | OperatingSystem::MacOSX { .. }
        ) {
            return Err("minimum OS version currently requires a macOS target".into());
        }
        self.min_os = Some(version);
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
        assert!(TargetSpec::parse("x86_64-unknown-linux-gnu")
            .unwrap()
            .with_min_os("13".parse().unwrap())
            .is_err());
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
