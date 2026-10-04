//! `[package].icon`: the PNG `dream` compiles into native builds (`--icon`), and the per-OS
//! bundle files `dreamer pack` writes next to the executable.

use crate::workspace::Workspace;
use anyhow::{bail, Context, Result};
use image::imageops::FilterType;
use image::RgbaImage;
use std::path::{Path, PathBuf};

const RECOMMENDED_SIZE: u32 = 256;

/// The package icon as an absolute path, after checking it exists and decodes as a PNG. Warns
/// when it is not square or smaller than 256x256, since OS icons are square and scaled up from it.
pub fn resolve(workspace: &Workspace) -> Result<Option<PathBuf>> {
    let Some(rel) = workspace.manifest.package()?.icon.clone() else {
        return Ok(None);
    };
    let path = workspace.root.join(&rel);
    let (w, h) = png_dimensions(&path).with_context(|| format!("package.icon = \"{rel}\""))?;
    if w != h {
        eprintln!("warning: package.icon {rel} is {w}x{h}; app icons should be square");
    }
    if w.min(h) < RECOMMENDED_SIZE {
        eprintln!(
            "warning: package.icon {rel} is {w}x{h}; use at least {RECOMMENDED_SIZE}x{RECOMMENDED_SIZE} so it stays sharp"
        );
    }
    Ok(Some(path))
}

fn png_dimensions(path: &Path) -> Result<(u32, u32)> {
    if !path.is_file() {
        bail!("{} does not exist", path.display());
    }
    let reader = image::ImageReader::open(path)
        .with_context(|| format!("reading {}", path.display()))?
        .with_guessed_format()?;
    if reader.format() != Some(image::ImageFormat::Png) {
        bail!("{} is not a PNG file", path.display());
    }
    Ok(reader.into_dimensions()?)
}

fn load_rgba(path: &Path) -> Result<RgbaImage> {
    Ok(image::open(path)
        .with_context(|| format!("decoding {}", path.display()))?
        .into_rgba8())
}

/// `icon.icns` with every standard size up to 512 px.
pub fn write_icns(png: &Path, out: &Path) -> Result<()> {
    let img = load_rgba(png)?;
    let mut family = icns::IconFamily::new();
    for size in [16u32, 32, 64, 128, 256, 512] {
        let scaled = image::imageops::resize(&img, size, size, FilterType::Lanczos3);
        let icon = icns::Image::from_data(icns::PixelFormat::RGBA, size, size, scaled.into_raw())
            .context("building icns image")?;
        family.add_icon(&icon).context("adding icns image")?;
    }
    let file = std::fs::File::create(out).with_context(|| format!("creating {}", out.display()))?;
    family
        .write(std::io::BufWriter::new(file))
        .with_context(|| format!("writing {}", out.display()))
}

/// Reverse-DNS bundle id: `dev.dream.<name>` with characters Apple disallows replaced by `-`.
pub fn bundle_id(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("dev.dream.{safe}")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn info_plist(name: &str, version: &str, executable: &str, has_icon: bool) -> String {
    let icon = if has_icon {
        "    <key>CFBundleIconFile</key>\n    <string>icon</string>\n"
    } else {
        ""
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>{exe}</string>
    <key>CFBundleIdentifier</key>
    <string>{id}</string>
    <key>CFBundleName</key>
    <string>{name}</string>
    <key>CFBundleDisplayName</key>
    <string>{name}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
    <key>CFBundleVersion</key>
    <string>{version}</string>
{icon}    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
"#,
        exe = xml_escape(executable),
        id = xml_escape(&bundle_id(name)),
        name = xml_escape(name),
        version = xml_escape(version),
    )
}

pub fn desktop_entry(name: &str, exe_name: &str, has_icon: bool) -> String {
    let icon = if has_icon {
        format!("Icon={name}\n")
    } else {
        String::new()
    };
    format!(
        "[Desktop Entry]\nType=Application\nName={name}\nExec={exe_name}\n{icon}Terminal=false\n"
    )
}

pub fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(path)?.permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(path, p)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::pack::{bundle::BundleWriter, desktop};

    fn write_png(path: &Path, w: u32, h: u32) {
        RgbaImage::from_pixel(w, h, image::Rgba([200, 40, 40, 255]))
            .save(path)
            .unwrap();
    }

    #[test]
    fn bundle_ids_are_reverse_dns() {
        assert_eq!(bundle_id("my_app"), "dev.dream.my-app");
        assert_eq!(bundle_id("tool-2"), "dev.dream.tool-2");
    }

    #[test]
    fn plist_names_the_icon_only_when_present() {
        let with = info_plist("demo", "1.2.0", "demo", true);
        assert!(with.contains("<string>dev.dream.demo</string>"));
        assert!(with.contains("<key>CFBundleIconFile</key>"));
        assert!(with.contains("<string>1.2.0</string>"));
        assert!(!info_plist("demo", "1.2.0", "demo", false).contains("CFBundleIconFile"));
        assert!(info_plist("a&b", "1", "a&b", false).contains("a&amp;b"));
    }

    #[test]
    fn icns_and_app_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let png = tmp.path().join("icon.png");
        write_png(&png, 300, 300);
        let bin = tmp.path().join("demo.bin");
        std::fs::write(&bin, b"binary").unwrap();
        let writer = BundleWriter::new(tmp.path()).unwrap();
        let app = desktop::write_macos_app(&writer, "demo", "0.1.0", &bin, Some(&png)).unwrap();
        let contents = app.join("Contents");
        assert!(contents.join("MacOS/demo").is_file());
        assert!(contents.join("Info.plist").is_file());
        let icns = std::fs::read(contents.join("Resources/icon.icns")).unwrap();
        assert_eq!(&icns[..4], b"icns");
        let family = icns::IconFamily::read(std::io::Cursor::new(icns)).unwrap();
        assert!(family.available_icons().len() >= 6);
    }

    #[test]
    fn desktop_entry_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let png = tmp.path().join("icon.png");
        write_png(&png, 256, 256);
        let writer = BundleWriter::new(tmp.path()).unwrap();
        let entry =
            desktop::write_linux_desktop(&writer, "demo", "demo-linux-x64", Some(&png)).unwrap();
        let text = std::fs::read_to_string(entry).unwrap();
        assert!(text.contains("Exec=demo-linux-x64\n"));
        assert!(text.contains("Icon=demo\n"));
        assert!(writer.root().join("demo.png").is_file());
    }

    #[test]
    fn rejects_missing_and_non_png() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(png_dimensions(&tmp.path().join("nope.png")).is_err());
        let fake = tmp.path().join("fake.png");
        std::fs::write(&fake, b"GIF89a....").unwrap();
        assert!(png_dimensions(&fake).is_err());
        let real = tmp.path().join("real.png");
        write_png(&real, 64, 32);
        assert_eq!(png_dimensions(&real).unwrap(), (64, 32));
    }
}
