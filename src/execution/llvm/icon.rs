//! `--icon`: the app icon PNG compiled into the binary. A small module holds the bytes and a
//! global constructor that hands them to the core host library (`dream_set_app_icon`) before `main`, so
//! windows and the macOS Dock use it without reading a file at run time. On Windows the same PNG
//! also becomes the `.exe` icon through a linked resource.

use std::fmt::Write as _;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Reads and validates the icon; a file that does not decode as PNG is a user error.
pub(super) fn read_png(icon: &Path) -> Result<Vec<u8>, String> {
    let png = std::fs::read(icon).map_err(|e| format!("icon {}: {e}", icon.display()))?;
    image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .map_err(|e| format!("icon {} is not a valid PNG: {e}", icon.display()))?;
    Ok(png)
}

/// A stable fingerprint for the build stamp, so changing the icon relinks.
pub(super) fn fingerprint(png: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    png.hash(&mut h);
    h.finish()
}

/// The `target` lines of the program module, so `llvm-link` sees matching modules.
fn target_header(program_ll: &str) -> String {
    program_ll
        .lines()
        .filter(|l| l.starts_with("target datalayout") || l.starts_with("target triple"))
        .fold(String::new(), |mut out, l| {
            out.push_str(l);
            out.push('\n');
            out
        })
}

fn c_string(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        if (0x20..0x7f).contains(&b) && b != b'"' && b != b'\\' {
            out.push(b as char);
        } else {
            let _ = write!(out, "\\{b:02X}");
        }
    }
    out
}

pub(super) fn icon_module(png: &[u8], program_ll: &str) -> String {
    let n = png.len();
    format!(
        "; Dream app icon\n{header}\n\
         @dream.app_icon = private unnamed_addr constant [{n} x i8] c\"{bytes}\", align 1\n\
         @llvm.global_ctors = appending global [1 x {{ i32, ptr, ptr }}] [{{ i32, ptr, ptr }} {{ i32 65535, ptr @dream.install_app_icon, ptr null }}]\n\n\
         declare void @dream_set_app_icon(ptr, i32)\n\n\
         define internal void @dream.install_app_icon() {{\n  \
           call void @dream_set_app_icon(ptr @dream.app_icon, i32 {n})\n  \
           ret void\n\
         }}\n",
        header = target_header(program_ll),
        bytes = c_string(png),
    )
}

/// Writes `<stem>.icon.ll` next to the program module and returns its path.
pub(super) fn write_icon_module(
    ll_path: &Path,
    png: &[u8],
    program_ll: &str,
) -> Result<PathBuf, String> {
    let out = ll_path.with_extension("icon.ll");
    std::fs::write(&out, icon_module(png, program_ll))
        .map_err(|e| format!("write {}: {e}", out.display()))?;
    Ok(out)
}

/// A multi-size `.ico` (16, 32, 48 and 256 px) for the Windows executable.
#[cfg(any(windows, test))]
pub(crate) fn png_to_ico(png: &[u8]) -> Result<Vec<u8>, String> {
    use image::codecs::ico::{IcoEncoder, IcoFrame};
    use image::imageops::FilterType;
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let mut frames = Vec::new();
    for size in [16u32, 32, 48, 256] {
        let scaled = image::imageops::resize(&img, size, size, FilterType::Lanczos3);
        frames.push(
            IcoFrame::as_png(scaled.as_raw(), size, size, image::ExtendedColorType::Rgba8)
                .map_err(|e| e.to_string())?,
        );
    }
    let mut out = Vec::new();
    IcoEncoder::new(&mut out)
        .encode_images(&frames)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// Compiles the icon into a `.res` with the pinned `llvm-rc`, for the Windows link.
#[cfg(windows)]
pub(super) fn windows_resource(
    tools: &super::tools::LlvmTools,
    ll_path: &Path,
    png: &[u8],
) -> Result<PathBuf, String> {
    let rc_tool = tools.optional_tool("llvm-rc")?;
    let ico = ll_path.with_extension("ico");
    std::fs::write(&ico, png_to_ico(png)?).map_err(|e| format!("write {}: {e}", ico.display()))?;
    let rc = ll_path.with_extension("rc");
    let ico_name = ico
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    std::fs::write(&rc, format!("1 ICON \"{ico_name}\"\n"))
        .map_err(|e| format!("write {}: {e}", rc.display()))?;
    let res = ll_path.with_extension("res");
    let mut cmd = std::process::Command::new(rc_tool);
    cmd.arg("/fo").arg(&res).arg(&rc);
    if let Some(dir) = rc.parent() {
        cmd.current_dir(dir);
    }
    crate::driver::wasi::run_captured(&mut cmd, "llvm-rc")?;
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROGRAM: &str = "; ModuleID = 'x'\ntarget datalayout = \"e-m:o\"\ntarget triple = \"arm64-apple-macosx\"\n\ndefine i32 @main() {\n  ret i32 0\n}\n";

    fn png(size: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(size, size, image::Rgba([10, 20, 30, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn icon_module_is_deterministic() {
        let bytes = png(8);
        let a = icon_module(&bytes, PROGRAM);
        assert_eq!(a, icon_module(&bytes, PROGRAM));
        assert!(a.contains("target triple = \"arm64-apple-macosx\""));
        assert!(a.contains(&format!("[{} x i8] c\"\\89PNG", bytes.len())));
        assert!(a.contains(&format!("i32 {})", bytes.len())));
        assert_eq!(fingerprint(&bytes), fingerprint(&bytes));
    }

    #[test]
    fn escapes_quotes_and_backslashes() {
        assert_eq!(c_string(b"a\"b\\c\n"), "a\\22b\\5Cc\\0A");
    }

    #[test]
    fn ico_holds_every_size() {
        let ico = png_to_ico(&png(300)).unwrap();
        let count = u16::from_le_bytes([ico[4], ico[5]]);
        assert_eq!(&ico[..4], &[0, 0, 1, 0]);
        assert_eq!(count, 4);
    }

    #[test]
    fn rejects_non_png() {
        let dir = std::env::temp_dir().join("dream_icon_reject.png");
        std::fs::write(&dir, b"not a png").unwrap();
        assert!(read_png(&dir).unwrap_err().contains("not a valid PNG"));
        let _ = std::fs::remove_file(&dir);
    }
}
