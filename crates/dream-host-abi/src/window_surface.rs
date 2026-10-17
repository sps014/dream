//! The native window description `dream-host-window` exports for GPU surface creation.
//!
//! A GPU capability links `dream_host_window` and calls [`WINDOW_SURFACE_SYMBOL`] with the
//! guest's window handle. It never calls GLFW: the window host owns the window and its thread.

use std::ffi::{c_ulong, c_void};

/// `extern "C" fn(window: i32, out: *mut WindowSurface) -> i32`: returns `1` and fills `out` for
/// a live native window, or `0` for headless, closed or unknown windows, platforms without a
/// native surface, and calls made off the window thread.
pub const WINDOW_SURFACE_SYMBOL: &str = "dream_window_surface_v1";

/// `window` is an `NSView*`; `display` is null.
pub const WINDOW_PLATFORM_APPKIT: u32 = 1;
/// `window` is an `HWND`; `display` is the owning `HINSTANCE`.
pub const WINDOW_PLATFORM_WIN32: u32 = 2;
/// `xlib_window` is the X11 `Window` id and `xlib_screen` its screen; `display` is `Display*`.
pub const WINDOW_PLATFORM_XLIB: u32 = 3;
/// `window` is a `wl_surface*`; `display` is `wl_display*`.
pub const WINDOW_PLATFORM_WAYLAND: u32 = 4;

/// Raw handles plus the drawable size, sampled at the time of the call. Handles stay valid until
/// the guest closes the window; re-query after every `Resized` event.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct WindowSurface {
    pub platform: u32,
    /// `1` when the window was created with `WindowDesc.vsync(true)`.
    pub vsync: u32,
    pub window: *mut c_void,
    pub display: *mut c_void,
    pub xlib_window: c_ulong,
    pub xlib_screen: i32,
    pub pixel_width: u32,
    pub pixel_height: u32,
    /// Physical pixels per logical pixel.
    pub scale: f32,
}

impl Default for WindowSurface {
    fn default() -> Self {
        Self {
            platform: 0,
            vsync: 0,
            window: std::ptr::null_mut(),
            display: std::ptr::null_mut(),
            xlib_window: 0,
            xlib_screen: 0,
            pixel_width: 0,
            pixel_height: 0,
            scale: 1.0,
        }
    }
}
