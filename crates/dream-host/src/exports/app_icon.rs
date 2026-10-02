//! C entry point for the compile-time app icon (`dream --icon`). The generated `icon.ll` module
//! calls it from a constructor, so it is not a stdlib `@runtime` host function.

use crate::app_icon;

/// # Safety
/// `png` must point to `len` bytes that live for the whole process (a constant global).
#[no_mangle]
pub unsafe extern "C" fn dream_set_app_icon(png: *const u8, len: i32) {
    if png.is_null() || len <= 0 {
        return;
    }
    app_icon::set_app_icon_png(std::slice::from_raw_parts(png, len as usize));
}
