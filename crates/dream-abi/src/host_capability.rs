//! Native host link artifacts, shared by the compiler and package manager.

#[path = "host_capability_fields.rs"]
mod fields;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostCapability {
    Core,
    Crypto,
    Process,
    Timezone,
    Net,
    Io,
    Window,
    Gpu,
    Audio,
}

impl HostCapability {
    pub const ALL: [Self; 9] = [
        Self::Core,
        Self::Crypto,
        Self::Process,
        Self::Timezone,
        Self::Net,
        Self::Io,
        Self::Window,
        Self::Gpu,
        Self::Audio,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Crypto => "crypto",
            Self::Process => "process",
            Self::Timezone => "timezone",
            Self::Net => "net",
            Self::Io => "io",
            Self::Window => "window",
            Self::Gpu => "gpu",
            Self::Audio => "audio",
        }
    }

    pub const fn link_name(self) -> &'static str {
        match self {
            Self::Core => "dream_host_core",
            Self::Crypto => "dream_host_crypto",
            Self::Process => "dream_host_process",
            Self::Timezone => "dream_host_timezone",
            Self::Net => "dream_host_net",
            Self::Io => "dream_host_io",
            Self::Window => "dream_host_window",
            Self::Gpu => "dream_host_gpu",
            Self::Audio => "dream_host_audio",
        }
    }

    pub fn abi_marker(self) -> String {
        let version = if self == Self::Net { 4 } else { 3 };
        format!("dream_host_{}_abi_v{version}", self.name())
    }

    pub fn fields(self) -> &'static [&'static str] {
        fields::fields(self)
    }

    pub fn for_import(module: &str, field: &str) -> Option<Self> {
        (module == crate::js_abi::HOST_MODULE)
            .then(|| {
                Self::ALL
                    .iter()
                    .copied()
                    .find(|c| c.fields().contains(&field))
            })
            .flatten()
    }

    pub fn required_for_imports<'a>(
        imports: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Vec<Self> {
        let selected: Vec<_> = imports
            .into_iter()
            .filter_map(|(module, field)| Self::for_import(module, field))
            .collect();
        Self::canonicalize(&selected)
    }

    fn canonicalize(selected: &[Self]) -> Vec<Self> {
        Self::ALL
            .iter()
            .copied()
            .filter(|c| selected.contains(c) || (*c == Self::Core && !selected.is_empty()))
            .collect()
    }

    pub fn library_name(self, target: &crate::target::TargetSpec) -> String {
        let name = self.link_name();
        if target.is_windows() {
            format!("{name}.dll")
        } else if target.is_apple() {
            format!("lib{name}.dylib")
        } else {
            format!("lib{name}.so")
        }
    }

    /// Shared libraries the capability library loads from its own directory at run time, so
    /// they ship next to it: the GPU host's Dawn (WebGPU) runtime.
    pub fn companion_library_names(
        self,
        target: &crate::target::TargetSpec,
    ) -> &'static [&'static str] {
        match self {
            Self::Gpu if target.is_windows() => &["dawn.dll"],
            Self::Gpu if target.is_apple() => &["libdawn.dylib"],
            Self::Gpu => &["libdawn.so"],
            _ => &[],
        }
    }

    pub fn import_library_name(self, target: &crate::target::TargetSpec) -> String {
        if target.is_msvc() {
            format!("{}.dll.lib", self.link_name())
        } else {
            format!("lib{}.dll.a", self.link_name())
        }
    }
}

/// The native linker and packager consume the same live-use inventory.
#[derive(serde::Deserialize)]
pub struct HostManifest {
    pub native_abi_version: u32,
    pub host_capabilities: Vec<HostCapability>,
}

impl HostManifest {
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        let manifest: Self = serde_json::from_str(json)?;
        if manifest.native_abi_version != 3 {
            return Err(<serde_json::Error as serde::de::Error>::custom(
                "stale native ABI manifest; rebuild with the capability ABI compiler (version 3)",
            ));
        }
        Ok(Self {
            native_abi_version: 3,
            host_capabilities: HostCapability::canonicalize(&manifest.host_capabilities),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_cookie_api_requires_its_updated_marker() {
        assert_eq!(HostCapability::Net.abi_marker(), "dream_host_net_abi_v4");
        assert_eq!(HostCapability::Core.abi_marker(), "dream_host_core_abi_v3");
    }

    #[test]
    fn manifest_requires_inventory_and_canonicalizes_order() {
        assert!(HostManifest::parse("{}").is_err());
        assert!(
            HostManifest::parse(r#"{"native_abi_version":3,"host_capabilities":["unknown"]}"#)
                .is_err()
        );
        let manifest = HostManifest::parse(
            r#"{"native_abi_version":3,"host_capabilities":["timezone","process","crypto","process"]}"#,
        )
        .unwrap();
        assert_eq!(
            manifest.host_capabilities,
            vec![
                HostCapability::Core,
                HostCapability::Crypto,
                HostCapability::Process,
                HostCapability::Timezone,
            ]
        );
    }

    #[test]
    fn manifest_rejects_missing_and_stale_native_abi_versions() {
        assert!(HostManifest::parse(r#"{"host_capabilities":["core"]}"#).is_err());
        for version in [0, 1, 2] {
            let json =
                format!("{{\"native_abi_version\":{version},\"host_capabilities\":[\"core\"]}}");
            let error = HostManifest::parse(&json).err().unwrap().to_string();
            assert!(error.contains("stale native ABI manifest"), "{}", error);
        }
        let manifest =
            HostManifest::parse(r#"{"native_abi_version":3,"host_capabilities":[]}"#).unwrap();
        assert_eq!(manifest.native_abi_version, 3);
        assert!(manifest.host_capabilities.is_empty());
    }
}
