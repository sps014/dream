use super::{Platform, capture, sdk};
use anyhow::{Context, Result, bail};
use dream_abi::host_capability::HostCapability;
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize)]
struct Check {
    name: String,
    available: bool,
    detail: String,
}

impl Check {
    fn new(name: impl Into<String>, result: Result<String>) -> Self {
        match result {
            Ok(detail) => Self {
                name: name.into(),
                available: true,
                detail,
            },
            Err(error) => Self {
                name: name.into(),
                available: false,
                detail: format!("{error:#}"),
            },
        }
    }
}

#[derive(Serialize)]
struct Report {
    platform: Platform,
    healthy: bool,
    checks: Vec<Check>,
}

pub fn run(platform: Platform, json: bool) -> Result<()> {
    let mut checks = Vec::new();
    let targets: &[&str] = match platform {
        Platform::Android => &["aarch64-linux-android", "x86_64-linux-android"],
        Platform::Ios => &["arm64-apple-ios", "arm64-apple-ios-simulator"],
    };
    let sysroot = match platform {
        Platform::Android => android(&mut checks),
        Platform::Ios => ios(&mut checks),
    };
    for target in targets {
        let result = compiler(target, sysroot.as_deref());
        checks.push(Check::new(format!("compiler/{target}"), result));
    }
    let report = Report {
        platform,
        healthy: checks.iter().all(|check| check.available),
        checks,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{} mobile prerequisites", platform.as_str());
        for check in &report.checks {
            let status = if check.available { "ok" } else { "missing" };
            println!("{status}: {}: {}", check.name, check.detail);
        }
        println!("GPU rendering and app execution require separate SDK-backed qualification.");
    }
    if !report.healthy {
        bail!("mobile toolchain diagnosis found missing requirements");
    }
    Ok(())
}

fn android(checks: &mut Vec<Check>) -> Option<PathBuf> {
    let root = sdk::android_root();
    checks.push(Check::new(
        "android-sdk",
        root.as_ref()
            .map(|path| path.display().to_string())
            .map_err(|e| anyhow::anyhow!("{e:#}")),
    ));
    checks.push(Check::new(
        "development-host",
        match std::env::consts::OS {
            "macos" | "linux" => Ok(std::env::consts::OS.into()),
            _ => Err(anyhow::anyhow!(
                "use macOS or Linux for Android app development"
            )),
        },
    ));
    for name in ["java", "javac", "keytool"] {
        checks.push(Check::new(
            name,
            which::which(name)
                .with_context(|| format!("install a JDK and add {name} to PATH"))
                .map(|path| path.display().to_string()),
        ));
    }
    checks.push(Check::new("jdk-version", jdk_version()));
    let root = root.ok()?;
    checks.push(Check::new(
        "adb",
        sdk::adb(&root).map(|p| p.display().to_string()),
    ));
    checks.push(Check::new("android-platform", android_platform(&root)));
    checks.push(Check::new("android-build-tools", build_tools(&root)));
    let ndk = sdk::ndk(&root).and_then(|path| sdk::ndk_clang(&path).map(|_| path));
    checks.push(Check::new(
        "android-ndk",
        ndk.as_ref()
            .map(|path| path.display().to_string())
            .map_err(|e| anyhow::anyhow!("{e:#}")),
    ));
    ndk.ok().map(|path| {
        path.join("toolchains/llvm/prebuilt")
            .join(if cfg!(target_os = "macos") {
                "darwin-x86_64"
            } else {
                "linux-x86_64"
            })
            .join("sysroot")
    })
}

fn jdk_version() -> Result<String> {
    let javac = which::which("javac").context("install JDK 21 or newer and add javac to PATH")?;
    let output = Command::new(javac)
        .arg("-version")
        .output()
        .context("checking javac version")?;
    if !output.status.success() {
        bail!(
            "javac cannot run; install JDK 21 or newer: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let version = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    validate_jdk(&version)?;
    Ok(version.trim().into())
}

fn validate_jdk(version: &str) -> Result<()> {
    let major = version
        .split_whitespace()
        .find_map(|word| word.split('.').next()?.parse::<u32>().ok());
    if major.is_none_or(|major| major < 21) {
        bail!(
            "JDK 21 or newer is required; javac reported {}",
            version.trim()
        );
    }
    Ok(())
}

fn android_platform(root: &Path) -> Result<String> {
    let mut installed = Vec::new();
    for entry in root
        .join("platforms")
        .read_dir()
        .context("install an Android SDK platform (API 26 or newer) with sdkmanager")?
    {
        let entry = entry?;
        if let Some(api) = entry
            .file_name()
            .to_string_lossy()
            .strip_prefix("android-")
            .and_then(|s| s.parse::<u32>().ok())
            && api >= 26
            && entry.path().join("android.jar").is_file()
        {
            installed.push((api, entry.path()));
        }
    }
    installed.sort();
    installed
        .pop()
        .map(|(_, path)| path.display().to_string())
        .context("install an Android SDK platform (API 26 or newer) with sdkmanager")
}

fn build_tools(root: &Path) -> Result<String> {
    let path = sdk::latest_version(&root.join("build-tools"))
        .context("install Android SDK build-tools with sdkmanager")?;
    for name in ["aapt2", "zipalign"] {
        sdk::require_file(
            path.join(sdk::executable(name)),
            "reinstall Android build-tools",
        )?;
    }
    for name in ["d8", "apksigner"] {
        let name = if cfg!(windows) {
            format!("{name}.bat")
        } else {
            name.into()
        };
        sdk::require_file(path.join(name), "reinstall Android build-tools")?;
    }
    Ok(path.display().to_string())
}

fn ios(checks: &mut Vec<Check>) -> Option<PathBuf> {
    for name in ["iphoneos", "iphonesimulator"] {
        checks.push(Check::new(
            name,
            sdk::ios_sdk(name).map(|p| p.display().to_string()),
        ));
    }
    if cfg!(target_os = "macos") {
        checks.push(Check::new(
            "xcodebuild",
            capture(Command::new("xcodebuild").arg("-version"))
                .context("install full Xcode and select it with xcode-select"),
        ));
        checks.push(Check::new(
            "simctl",
            capture(Command::new("xcrun").args(["simctl", "list", "runtimes", "--json"]))
                .and_then(|output| simulator_runtime(&output)),
        ));
    }
    // Each compiler target needs its own Apple SDK, selected in `compiler`.
    None
}

fn simulator_runtime(output: &str) -> Result<String> {
    let value: Value = serde_json::from_str(output).context("reading simctl runtimes")?;
    let runtimes = value["runtimes"]
        .as_array()
        .context("simctl omitted runtimes")?;
    runtimes
        .iter()
        .find(|runtime| {
            runtime["isAvailable"].as_bool() == Some(true)
                && runtime["identifier"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("com.apple.CoreSimulator.SimRuntime.iOS-"))
        })
        .map(|runtime| {
            runtime["identifier"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .context("install an iOS simulator runtime in Xcode Settings > Components")
}

fn compiler(target: &str, android_sysroot: Option<&Path>) -> Result<String> {
    let mut command = Command::new(crate::dream_bin::locate()?);
    command
        .args(["toolchain-doctor", "--target", target, "--json"])
        .env("DREAM_NO_AUTO_INSTALL", "1");
    let apple_sdk;
    if target.contains("apple-ios") {
        apple_sdk = sdk::ios_sdk(if target.ends_with("simulator") {
            "iphonesimulator"
        } else {
            "iphoneos"
        })?;
        command.env("DREAM_SYSROOT", &apple_sdk);
    } else if let Some(path) = android_sysroot {
        command.env("DREAM_SYSROOT", path);
    }
    let output = command
        .output()
        .context("running compiler toolchain doctor")?;
    let value: Value = serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "reading compiler diagnosis: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    })?;
    validate_compiler(&value, output.status.success())
}

fn validate_compiler(value: &Value, success: bool) -> Result<String> {
    let mut errors = value["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    for capability in [
        HostCapability::Core,
        HostCapability::Window,
        HostCapability::Gpu,
        HostCapability::Drawing,
        HostCapability::App,
    ] {
        if value["tools"][capability.link_name()]["available"].as_bool() != Some(true) {
            errors.push(format!(
                "missing target-built {} capability library",
                capability.name()
            ));
        }
    }
    if !success || value["healthy"].as_bool() != Some(true) || !errors.is_empty() {
        bail!(
            "{}",
            if errors.is_empty() {
                "compiler toolchain diagnosis failed".into()
            } else {
                errors.join("; ")
            }
        );
    }
    Ok("compiler tools and core/window/gpu/drawing/app target libraries found".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn java_version_must_be_21_or_newer() {
        assert!(validate_jdk("javac 21.0.8\n").is_ok());
        assert!(validate_jdk("javac 25\n").is_ok());
        assert!(validate_jdk("javac 17.0.1\n").is_err());
        assert!(validate_jdk("javac 1.8.0_402\n").is_err());
        assert!(validate_jdk("unknown\n").is_err());
    }

    #[test]
    fn core_only_diagnosis_does_not_claim_canvas_support() {
        let report = json!({"healthy":true,"errors":[],"tools":{
            "dream_host_core":{"available":true}
        }});
        let error = validate_compiler(&report, true).unwrap_err().to_string();
        for capability in ["window", "gpu", "drawing", "app"] {
            assert!(error.contains(capability));
        }
    }

    #[test]
    fn simulator_runtime_requires_available_ios_runtime() {
        for runtime in [
            "[]",
            r#"[{"identifier":"com.apple.CoreSimulator.SimRuntime.watchOS-11-0","isAvailable":true}]"#,
            r#"[{"identifier":"com.apple.CoreSimulator.SimRuntime.iOS-18-0","isAvailable":false}]"#,
        ] {
            assert!(simulator_runtime(&format!(r#"{{"runtimes":{runtime}}}"#)).is_err());
        }
        assert!(simulator_runtime(r#"{"runtimes":[{"identifier":"com.apple.CoreSimulator.SimRuntime.iOS-18-0","isAvailable":true}]}"#).is_ok());
    }

    #[test]
    fn platform_requires_api_26_and_android_jar() {
        let root = tempfile::tempdir().unwrap();
        for api in [25, 26, 35, 36] {
            let dir = root.path().join(format!("platforms/android-{api}"));
            std::fs::create_dir_all(&dir).unwrap();
            if api != 36 {
                std::fs::write(dir.join("android.jar"), "").unwrap();
            }
        }
        assert!(
            android_platform(root.path())
                .unwrap()
                .ends_with("android-35")
        );
    }
}
