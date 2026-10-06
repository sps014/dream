//! Native host link artifacts, shared by the compiler and package manager.

#[path = "host_capability_fields.rs"]
mod fields;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostCapability {
    Core,
    Unicode,
    Crypto,
    Process,
    Timezone,
}

impl HostCapability {
    pub const ALL: [Self; 5] = [
        Self::Core,
        Self::Unicode,
        Self::Crypto,
        Self::Process,
        Self::Timezone,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Unicode => "unicode",
            Self::Crypto => "crypto",
            Self::Process => "process",
            Self::Timezone => "timezone",
        }
    }

    pub const fn link_name(self) -> &'static str {
        match self {
            Self::Core => "dream_host_core",
            Self::Unicode => "dream_host_unicode",
            Self::Crypto => "dream_host_crypto",
            Self::Process => "dream_host_process",
            Self::Timezone => "dream_host_timezone",
        }
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
        if manifest.native_abi_version != 2 {
            return Err(<serde_json::Error as serde::de::Error>::custom(
                "stale native ABI manifest; rebuild with the pointer ABI compiler (version 2)",
            ));
        }
        Ok(Self {
            native_abi_version: 2,
            host_capabilities: HostCapability::canonicalize(&manifest.host_capabilities),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_requires_inventory_and_canonicalizes_order() {
        assert!(HostManifest::parse("{}").is_err());
        assert!(
            HostManifest::parse(r#"{"native_abi_version":2,"host_capabilities":["unknown"]}"#)
                .is_err()
        );
        let manifest = HostManifest::parse(
            r#"{"native_abi_version":2,"host_capabilities":["timezone","unicode","crypto","unicode"]}"#,
        )
        .unwrap();
        assert_eq!(
            manifest.host_capabilities,
            vec![
                HostCapability::Core,
                HostCapability::Unicode,
                HostCapability::Crypto,
                HostCapability::Timezone,
            ]
        );
    }

    #[test]
    fn manifest_rejects_missing_and_stale_native_abi_versions() {
        assert!(HostManifest::parse(r#"{"host_capabilities":["core"]}"#).is_err());
        for version in [0, 1, 3] {
            let json =
                format!("{{\"native_abi_version\":{version},\"host_capabilities\":[\"core\"]}}");
            let error = HostManifest::parse(&json).err().unwrap().to_string();
            assert!(error.contains("stale native ABI manifest"), "{}", error);
        }
        let manifest =
            HostManifest::parse(r#"{"native_abi_version":2,"host_capabilities":[]}"#).unwrap();
        assert_eq!(manifest.native_abi_version, 2);
        assert!(manifest.host_capabilities.is_empty());
    }
}
