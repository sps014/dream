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

pub(super) fn ndk_clang(root: &Path) -> Result<PathBuf> {
    let host = match std::env::consts::OS {
        "macos" => "darwin-x86_64",
        "linux" => "linux-x86_64",
        other => bail!("mobile app development on {other} is not supported; use macOS or Linux"),
    };
    require_file(
        root.join("toolchains/llvm/prebuilt")
            .join(host)
            .join("bin/clang"),
        "install the NDK for this development host",
    )
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
    fn missing_adb_explains_how_to_install_it() {
        let root = tempfile::tempdir().unwrap();
        let error = adb(root.path()).unwrap_err().to_string();
        assert!(error.contains("platform-tools"));
        assert!(error.contains("sdkmanager"));
    }
}
