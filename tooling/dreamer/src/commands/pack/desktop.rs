use super::bundle::BundleWriter;
use crate::app_icon::{desktop_entry, info_plist, make_executable, write_icns};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// `<Name>.app/Contents/{MacOS/<name>, Info.plist, Resources/icon.icns}` in `pack_dir`.
pub fn write_macos_app(
    writer: &BundleWriter,
    name: &str,
    version: &str,
    bin: &Path,
    icon: Option<&Path>,
) -> Result<PathBuf> {
    let app = writer.path(format!("{name}.app"))?;
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    std::fs::create_dir_all(&macos).with_context(|| format!("creating {}", macos.display()))?;
    let exe = macos.join(name);
    writer.copy(
        bin,
        Path::new(&format!("{name}.app"))
            .join("Contents/MacOS")
            .join(name),
    )?;
    make_executable(&exe)?;
    if let Some(icon) = icon {
        let resources = contents.join("Resources");
        std::fs::create_dir_all(&resources)?;
        write_icns(icon, &resources.join("icon.icns"))?;
    }
    std::fs::write(
        contents.join("Info.plist"),
        info_plist(name, version, name, icon.is_some()),
    )?;
    Ok(app)
}

/// `<name>.desktop` (and `<name>.png`) next to the packed Linux executable.
pub fn write_linux_desktop(
    writer: &BundleWriter,
    name: &str,
    exe_name: &str,
    icon: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(icon) = icon {
        writer
            .copy(icon, format!("{name}.png"))
            .with_context(|| format!("copying {}", icon.display()))?;
    }
    let entry = writer.path(format!("{name}.desktop"))?;
    std::fs::write(&entry, desktop_entry(name, exe_name, icon.is_some()))?;
    Ok(entry)
}
