//! C entry point for the compile-time app icon (`dream --icon`). The generated `icon.ll` module
//! calls it from a constructor, so it is not a stdlib `@runtime` host function.

use std::sync::OnceLock;

static APP_ICON_PNG: OnceLock<&'static [u8]> = OnceLock::new();

/// # Safety
/// `png` must point to `len` bytes that live for the whole process (a constant global).
#[no_mangle]
pub unsafe extern "C" fn dream_set_app_icon(png: *const u8, len: i32) {
    if png.is_null() || len <= 0 {
        return;
    }
    let _ = APP_ICON_PNG.set(std::slice::from_raw_parts(png, len as usize));
}

/// # Safety
/// `length` must point to a writable usize.
#[no_mangle]
pub unsafe extern "C" fn dream_host_app_icon(length: *mut usize) -> *const u8 {
    match APP_ICON_PNG.get() {
        Some(png) => {
            length.write(png.len());
            png.as_ptr()
        }
        None => {
            length.write(0);
            std::ptr::null()
        }
    }
}
