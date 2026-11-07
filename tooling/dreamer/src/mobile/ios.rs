//! iOS simulator app bundles: the guest executable, capability dylibs under `Frameworks/`, and
//! an `Info.plist`, ad-hoc signed and run through `simctl`.

use super::app::Guest;
use super::capture;
use crate::app_config::AppConfig;
use crate::workspace::Workspace;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn package(workspace: &Workspace, app: &AppConfig, guest: &Guest) -> Result<PathBuf> {
    let package = workspace.manifest.package()?;
    let executable = package.name.clone();
    let bundle = guest.dir.join(format!("{}.app", package.name));
    if bundle.exists() {
        std::fs::remove_dir_all(&bundle)
            .with_context(|| format!("clearing {}", bundle.display()))?;
    }
    let frameworks = bundle.join("Frameworks");
    std::fs::create_dir_all(&frameworks)?;
    std::fs::copy(&guest.artifact, bundle.join(&executable))
        .with_context(|| format!("copying {}", guest.artifact.display()))?;
    let mut signed = Vec::new();
    for name in guest.runtime_files() {
        let source = guest.dir.join(&name);
        let destination = if name.ends_with(".dylib") {
            frameworks.join(&name)
        } else {
            bundle.join(&name)
        };
        std::fs::copy(&source, &destination)
            .with_context(|| format!("copying staged capability file {name}"))?;
        if name.ends_with(".dylib") {
            signed.push(destination);
        }
    }
    std::fs::write(
        bundle.join("Info.plist"),
        info_plist(app, &package.name, &package.version, &executable),
    )?;
    crate::assets::copy_into(&workspace.root, &package.assets, &bundle)?;
    // The simulator only requires an ad-hoc signature; nested code is signed before its bundle.
    signed.push(bundle.clone());
    for path in &signed {
        capture(
            Command::new("codesign")
                .args(["--force", "--sign", "-", "--timestamp=none"])
                .arg(path),
        )
        .with_context(|| format!("signing {}", path.display()))?;
    }
    Ok(bundle)
}

pub(super) fn install_and_launch(udid: &str, id: &str, bundle: &Path) -> Result<()> {
    let simctl = |args: &[&str]| {
        let mut command = Command::new("xcrun");
        command.arg("simctl").args(args);
        command
    };
    capture(&mut simctl(&["bootstatus", udid, "-b"])).context("booting the simulator")?;
    capture(simctl(&["install", udid]).arg(bundle)).context("installing the app")?;
    println!("launching {id}; its output follows, press Ctrl-C to stop");
    let status = simctl(&[
        "launch",
        "--console-pty",
        "--terminate-running-process",
        udid,
        id,
    ])
    .status()
    .context("launching the app")?;
    if !status.success() {
        bail!(
            "the app exited with a failure ({})",
            crate::process_status::describe(&status)
        );
    }
    Ok(())
}

fn info_plist(app: &AppConfig, name: &str, version: &str, executable: &str) -> String {
    let entries = [
        (
            "CFBundleDevelopmentRegion",
            "<string>en</string>".to_string(),
        ),
        ("CFBundleDisplayName", string(name)),
        ("CFBundleExecutable", string(executable)),
        ("CFBundleIdentifier", string(&app.id)),
        ("CFBundleInfoDictionaryVersion", string("6.0")),
        ("CFBundleName", string(name)),
        ("CFBundlePackageType", string("APPL")),
        ("CFBundleShortVersionString", string(version)),
        (
            "CFBundleSupportedPlatforms",
            "<array><string>iPhoneSimulator</string></array>".into(),
        ),
        ("CFBundleVersion", string("1")),
        ("LSRequiresIPhoneOS", "<true/>".into()),
        ("MinimumOSVersion", string(&app.ios_min_os)),
        (
            "UIDeviceFamily",
            "<array><integer>1</integer><integer>2</integer></array>".into(),
        ),
        ("UILaunchScreen", "<dict/>".into()),
        ("UIRequiresFullScreen", "<true/>".into()),
    ];
    let body = entries
        .iter()
        .map(|(key, value)| format!("\t<key>{key}</key>\n\t{value}\n"))
        .collect::<String>();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n{body}</dict>\n</plist>\n"
    )
}

fn string(text: &str) -> String {
    format!(
        "<string>{}</string>",
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_plist_names_the_executable_identity_and_minimum() {
        let mut app = AppConfig::new("demo");
        app.ios_min_os = "17.0".into();
        let plist = info_plist(&app, "a&b", "1.2.3", "demo");
        assert!(plist.contains("<key>CFBundleExecutable</key>\n\t<string>demo</string>"));
        assert!(plist.contains(&format!("<string>{}</string>", app.id)));
        assert!(plist.contains("<key>MinimumOSVersion</key>\n\t<string>17.0</string>"));
        assert!(plist.contains("<string>a&amp;b</string>"));
        assert!(plist.contains("<string>iPhoneSimulator</string>"));
    }
}
