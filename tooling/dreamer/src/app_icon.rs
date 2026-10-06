//! Validation for the PNG icon embedded in native executables.

use crate::workspace::Workspace;
use anyhow::{bail, Context, Result};
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

    fn write_png(path: &Path, w: u32, h: u32) {
        image::RgbaImage::from_pixel(w, h, image::Rgba([200, 40, 40, 255]))
            .save(path)
            .unwrap();
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
