use anyhow::{Context, Result, bail};
use dream_abi::target::OsVersion;
use serde::{Deserialize, Serialize};

/// Identity and deployment minimums shared by the mobile application shells.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AppConfig {
    pub id: String,
    #[serde(default = "android_min_sdk")]
    pub android_min_sdk: u32,
    #[serde(default = "ios_min_os")]
    pub ios_min_os: String,
}

fn android_min_sdk() -> u32 {
    26
}
fn ios_min_os() -> String {
    "16.0".into()
}

impl AppConfig {
    pub fn new(name: &str) -> Self {
        let suffix = name
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase();
        Self {
            id: format!("org.example.app{suffix}"),
            android_min_sdk: android_min_sdk(),
            ios_min_os: ios_min_os(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        crate::identifiers::java_package(&self.id).context("invalid app.id")?;
        if self.id.split('.').count() < 2 || self.id.contains('_') {
            bail!(
                "app.id must have at least two dot-separated segments and contain only letters, digits, and dots for Android/iOS"
            );
        }
        if self.android_min_sdk < 26 {
            bail!("app.android-min-sdk must be at least 26");
        }
        let version = self
            .ios_min_os
            .parse::<OsVersion>()
            .map_err(anyhow::Error::msg)
            .context("invalid app.ios-min-os")?;
        if version.major < 16 {
            bail!("app.ios-min-os must be at least 16.0");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_identity_works_for_every_package_name_shape() {
        for name in ["hello", "my-app", "my_app", "My.App", "x1", "class", "int"] {
            AppConfig::new(name).validate().unwrap();
        }
    }

    #[test]
    fn invalid_platform_identity_and_minimums_are_rejected() {
        for id in [
            "single",
            "org.class",
            "org.my_app",
            "org.my-app",
            "org.1app",
            "org..app",
        ] {
            let mut config = AppConfig::new("demo");
            config.id = id.into();
            assert!(config.validate().is_err(), "{id}");
        }
        let mut config = AppConfig::new("demo");
        config.android_min_sdk = 25;
        assert!(config.validate().is_err());
        config.android_min_sdk = 26;
        for version in ["15.9", "16.x", "16.0.0.0", ""] {
            config.ios_min_os = version.into();
            assert!(config.validate().is_err(), "{version}");
        }
    }
}
