//! App window icon: packaged PNG bytes (single-file `dreamer pack`) or a linked
//! `[package].icon` path next to `dream.toml` (`dream run` from a project tree).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use winit::window::Icon;

static PACKAGED_ICON_PNG: OnceLock<&'static [u8]> = OnceLock::new();

/// Install PNG bytes baked into a packed `dream-runner` executable. Call once before running wasm.
pub fn set_packaged_app_icon(png: &'static [u8]) {
    if png.is_empty() {
        return;
    }
    let _ = PACKAGED_ICON_PNG.set(png);
}

/// Prefer packaged (single-file) icon; otherwise load `[package].icon` from disk.
pub fn load_window_icon() -> Option<Icon> {
    if let Some(bytes) = PACKAGED_ICON_PNG.get().copied() {
        return match icon_from_png_bytes(bytes) {
            Ok(icon) => Some(icon),
            Err(e) => {
                eprintln!("Dream: failed to decode packaged app icon: {e}");
                None
            }
        };
    }
    let path = resolve_icon_path()?;
    match icon_from_png_path(&path) {
        Ok(icon) => Some(icon),
        Err(e) => {
            eprintln!(
                "Dream: failed to load package.icon '{}': {e}",
                path.display()
            );
            None
        }
    }
}

fn resolve_icon_path() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    icon_path_from_project(&cwd)
}

fn icon_path_from_project(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        let manifest = dir.join("dream.toml");
        if manifest.is_file() {
            let rel = parse_package_icon(&manifest)?;
            let path = dir.join(rel);
            return path.is_file().then_some(path);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Minimal `[package]` `icon = "..."` reader (avoids tying the compiler host to dreamer's Manifest).
fn parse_package_icon(manifest: &Path) -> Option<String> {
    let text = std::fs::read_to_string(manifest).ok()?;
    parse_package_icon_text(&text)
}

fn parse_package_icon_text(text: &str) -> Option<String> {
    let mut in_package = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("icon") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim();
        let quote = rest.chars().next()?;
        if quote != '"' && quote != '\'' {
            continue;
        }
        let end = rest[1..].find(quote)?;
        let value = rest[1..1 + end].to_string();
        if value.is_empty()
            || Path::new(&value).is_absolute()
            || value.split('/').any(|p| p == "..")
        {
            return None;
        }
        return Some(value);
    }
    None
}

fn icon_from_png_path(path: &Path) -> Result<Icon, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    icon_from_png_bytes(&bytes)
}

fn icon_from_png_bytes(bytes: &[u8]) -> Result<Icon, String> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use std::io::Cursor;

    #[test]
    fn parses_package_icon_line() {
        let text = "[package]\nname = \"t\"\nicon = \"assets/icon.png\"\n";
        assert_eq!(
            parse_package_icon_text(text).as_deref(),
            Some("assets/icon.png")
        );
        assert!(parse_package_icon_text("[package]\nicon = \"../x.png\"\n").is_none());
    }

    #[test]
    fn decodes_png_bytes_to_icon() {
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(2, 2, Rgba([255, 0, 0, 255]));
        let mut png = Vec::new();
        img.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        assert!(icon_from_png_bytes(&png).is_ok());
    }
}
