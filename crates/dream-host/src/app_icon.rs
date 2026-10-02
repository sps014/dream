//! App icon shared by GPU and WebView windows (and the macOS Dock).
//!
//! The PNG is baked into the program at compile time (`dream --icon`, which `dreamer` passes from
//! `[package].icon`): a module constructor hands the bytes to [`set_app_icon_png`] before `main`.

use std::sync::OnceLock;

use winit::window::Icon;

static APP_ICON_PNG: OnceLock<&'static [u8]> = OnceLock::new();

pub(crate) fn set_app_icon_png(png: &'static [u8]) {
    if !png.is_empty() {
        let _ = APP_ICON_PNG.set(png);
    }
}

pub(crate) fn app_icon_png() -> Option<&'static [u8]> {
    APP_ICON_PNG.get().copied()
}

/// The compiled-in icon, decoded for a winit window.
pub(crate) fn window_icon() -> Option<Icon> {
    let bytes = app_icon_png()?;
    match icon_from_png_bytes(bytes) {
        Ok(icon) => Some(icon),
        Err(e) => {
            eprintln!("Dream: failed to decode app icon: {e}");
            None
        }
    }
}

pub(crate) fn icon_from_png_bytes(bytes: &[u8]) -> Result<Icon, String> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).map_err(|e| e.to_string())
}

/// macOS shows the Dock icon from the app bundle; a bare binary needs it set on NSApplication.
/// Call on the main thread after the event loop exists.
#[cfg(all(target_os = "macos", feature = "webview"))]
pub(crate) fn apply_dock_icon(png: &[u8]) {
    use objc2::AllocAnyThread;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(png);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    unsafe { app.setApplicationIconImage(Some(&image)) };
}

#[cfg(not(all(target_os = "macos", feature = "webview")))]
pub(crate) fn apply_dock_icon(_png: &[u8]) {}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use std::io::Cursor;

    #[test]
    fn decodes_png_bytes_to_icon() {
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(2, 2, Rgba([255, 0, 0, 255]));
        let mut png = Vec::new();
        img.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        assert!(icon_from_png_bytes(&png).is_ok());
        assert!(icon_from_png_bytes(b"not a png").is_err());
    }
}
