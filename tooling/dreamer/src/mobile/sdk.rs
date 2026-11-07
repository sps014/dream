use anyhow::{Context, Result, bail};
use semver::Version;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn android_root() -> Result<PathBuf> {
    let root = std::env::var_os("ANDROID_HOME")
        .or_else(|| std::env::var_os("ANDROID_SDK_ROOT"))
        .map(PathBuf::from)
        .context("Android SDK not configured; set ANDROID_HOME to the installed SDK directory")?;
    if !root.is_dir() {
        bail!("Android SDK directory does not exist: {}", root.display());
    }
    Ok(root)
}

pub(super) fn adb(root: &Path) -> Result<PathBuf> {
    require_file(
        root.join("platform-tools").join(executable("adb")),
        "install Android SDK platform-tools with sdkmanager",
    )
}

pub(super) fn executable(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

pub(super) fn require_file(path: PathBuf, advice: &str) -> Result<PathBuf> {
    if !path.is_file() {
        bail!("missing {}; {advice}", path.display());
    }
    Ok(path)
}

pub(super) fn latest_version(directory: &Path) -> Result<PathBuf> {
    let mut versions = Vec::new();
    for entry in directory
        .read_dir()
        .with_context(|| format!("reading {}", directory.display()))?
    {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && let Ok(version) = Version::parse(&entry.file_name().to_string_lossy())
            && version.pre.is_empty()
        {
            versions.push((version, entry.path()));
        }
    }
    versions.sort();
    versions.pop().map(|(_, path)| path).with_context(|| {
        format!(
            "no stable SDK component installed in {}",
            directory.display()
        )
    })
}

pub(super) fn ndk(root: &Path) -> Result<PathBuf> {
    match std::env::var_os("ANDROID_NDK_HOME") {
        Some(path) => Ok(PathBuf::from(path)),
        None => latest_version(&root.join("ndk"))
            .context("install an Android NDK with sdkmanager or set ANDROID_NDK_HOME"),
    }
}

/// The NDK's LLVM prebuilt directory for this development machine.
///
/// Apple Silicon NDKs ship `darwin-arm64`. Older NDKs only ship `darwin-x86_64`, which still
/// runs under Rosetta, so that directory is the fallback.
pub(super) fn ndk_prebuilt(root: &Path) -> Result<PathBuf> {
    let hosts: &[&str] = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => &["darwin-arm64", "darwin-x86_64"],
        ("macos", _) => &["darwin-x86_64", "darwin-arm64"],
        ("linux", _) => &["linux-x86_64"],
        (os, _) => {
            bail!("mobile app development on {os} is not supported; use macOS or Linux")
        }
    };
    for host in hosts {
        let prebuilt = root.join("toolchains/llvm/prebuilt").join(host);
        if prebuilt.join("bin").join(executable("clang")).is_file() {
            return Ok(prebuilt);
        }
    }
    bail!(
        "missing NDK clang under {}; install the NDK for this development host",
        root.join("toolchains/llvm/prebuilt").display()
    )
}

/// The newest installed SDK platform at or above `minimum`, as `(api, android.jar)`.
pub(super) fn android_platform(root: &Path, minimum: u32) -> Result<(u32, PathBuf)> {
    let advice = || {
        format!(
            "install Android SDK platform {minimum} or newer with sdkmanager \"platforms;android-{minimum}\""
        )
    };
    let mut installed = Vec::new();
    for entry in root.join("platforms").read_dir().with_context(advice)? {
        let entry = entry?;
        if let Some(api) = entry
            .file_name()
            .to_string_lossy()
            .strip_prefix("android-")
            .and_then(|s| s.parse::<u32>().ok())
            && api >= minimum
            && entry.path().join("android.jar").is_file()
        {
            installed.push((api, entry.path().join("android.jar")));
        }
    }
    installed.sort();
    installed.pop().with_context(advice)
}

pub(super) fn ios_sdk(name: &str) -> Result<PathBuf> {
    if !cfg!(target_os = "macos") {
        bail!("iOS development requires macOS and full Xcode");
    }
    let output = super::capture(Command::new("xcrun").args(["--sdk", name, "--show-sdk-path"]))
        .context("install full Xcode and select it with xcode-select; command-line tools alone do not include iOS SDKs")?;
    let path = PathBuf::from(output.trim());
    if !path.is_dir() {
        bail!("Xcode returned a missing {name} SDK: {}", path.display());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_selection_is_numeric_and_ignores_previews_and_files() {
        let root = tempfile::tempdir().unwrap();
        for version in ["9.0.0", "10.0.0", "11.0.0-rc.1", "not-a-version"] {
            std::fs::create_dir(root.path().join(version)).unwrap();
        }
        std::fs::write(root.path().join("20.0.0"), "").unwrap();
        assert_eq!(
            latest_version(root.path()).unwrap(),
            root.path().join("10.0.0")
        );
    }

    #[test]
    fn empty_component_directory_is_an_error() {
        let root = tempfile::tempdir().unwrap();
        assert!(latest_version(root.path()).is_err());
    }

    #[test]
    fn ndk_prebuilt_prefers_the_native_host_and_falls_back() {
        let root = tempfile::tempdir().unwrap();
        let native = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            "darwin-arm64"
        } else if cfg!(target_os = "macos") {
            "darwin-x86_64"
        } else {
            "linux-x86_64"
        };
        let fallback = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            "darwin-x86_64"
        } else {
            native
        };
        let clang = root
            .path()
            .join("toolchains/llvm/prebuilt")
            .join(fallback)
            .join("bin")
            .join(executable("clang"));
        std::fs::create_dir_all(clang.parent().unwrap()).unwrap();
        std::fs::write(&clang, "").unwrap();
        assert_eq!(
            ndk_prebuilt(root.path()).unwrap(),
            clang.parent().unwrap().parent().unwrap()
        );
        if fallback != native {
            let native_clang = root
                .path()
                .join("toolchains/llvm/prebuilt")
                .join(native)
                .join("bin")
                .join(executable("clang"));
            std::fs::create_dir_all(native_clang.parent().unwrap()).unwrap();
            std::fs::write(&native_clang, "").unwrap();
            assert_eq!(
                ndk_prebuilt(root.path()).unwrap(),
                native_clang.parent().unwrap().parent().unwrap()
            );
        }
    }

    #[test]
    fn platform_requires_the_minimum_api_and_android_jar() {
        let root = tempfile::tempdir().unwrap();
        for api in [26, 34, 35, 36] {
            let dir = root.path().join(format!("platforms/android-{api}"));
            std::fs::create_dir_all(&dir).unwrap();
            if api != 36 {
                std::fs::write(dir.join("android.jar"), "").unwrap();
            }
        }
        let (api, jar) = android_platform(root.path(), 35).unwrap();
        assert_eq!(api, 35);
        assert!(jar.ends_with("android-35/android.jar"));
        let error = android_platform(root.path(), 37).unwrap_err().to_string();
        assert!(error.contains("platforms;android-37"));
    }

    #[test]
    fn missing_adb_explains_how_to_install_it() {
        let root = tempfile::tempdir().unwrap();
        let error = adb(root.path()).unwrap_err().to_string();
        assert!(error.contains("platform-tools"));
        assert!(error.contains("sdkmanager"));
    }
}
