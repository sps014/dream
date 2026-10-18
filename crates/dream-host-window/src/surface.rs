//! Host-to-host C ABI for GPU surfaces; guests never import it. The layout and symbol name live
//! in `dream_host_abi::window_surface` so the GPU capability shares one definition.

use dream_host_abi::window_surface::WindowSurface;

/// # Safety
/// `out` must be null or point to writable `WindowSurface` storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dream_window_surface_v1(window: i32, out: *mut WindowSurface) -> i32 {
    if out.is_null() {
        return 0;
    }
    match crate::host::surface(window) {
        Some(surface) => {
            unsafe { out.write(surface) };
            1
        }
        None => 0,
    }
}
