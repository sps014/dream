//! Native host link artifacts, shared by the compiler and package manager.

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostCapability {
    Core,
    Net,
    Gpu,
    WebView,
}

impl HostCapability {
    pub const ALL: [Self; 4] = [Self::Core, Self::Net, Self::Gpu, Self::WebView];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Net => "net",
            Self::Gpu => "gpu",
            Self::WebView => "webview",
        }
    }

    pub const fn link_name(self) -> &'static str {
        match self {
            Self::Core => "dream_host_core",
            Self::Net => "dream_host_net",
            Self::Gpu => "dream_host_gpu",
            Self::WebView => "dream_host_webview",
        }
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
            // Core binds guest callbacks even when no stdlib extern survives pruning.
            host_capabilities: HostCapability::ALL
                .iter()
                .copied()
                .filter(|c| *c == HostCapability::Core || manifest.host_capabilities.contains(c))
                .collect(),
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
            r#"{"native_abi_version":2,"host_capabilities":["webview","gpu","net","gpu"]}"#,
        )
        .unwrap();
        assert_eq!(manifest.host_capabilities, HostCapability::ALL);
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
        assert_eq!(manifest.host_capabilities, vec![HostCapability::Core]);
    }
}
