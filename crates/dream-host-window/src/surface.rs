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

pub(crate) fn native(window: &sdl3::video::Window, vsync: bool) -> Option<WindowSurface> {
    use sdl3::sys::{properties, video};
    let metrics = crate::metrics::Metrics::read(window);
    // The SDL window remains owned by the main-thread host for the lifetime of these handles.
    let props = unsafe { video::SDL_GetWindowProperties(window.raw()) };
    if props.0 == 0 {
        return None;
    }
    let mut out = WindowSurface {
        vsync: u32::from(vsync),
        pixel_width: metrics.pixels.0,
        pixel_height: metrics.pixels.1,
        scale: metrics.scale,
        ..WindowSurface::default()
    };
    let pointer =
        |key| unsafe { properties::SDL_GetPointerProperty(props, key, std::ptr::null_mut()) };
    #[cfg(target_os = "macos")]
    {
        let nswindow = pointer(video::SDL_PROP_WINDOW_COCOA_WINDOW_POINTER);
        if nswindow.is_null() {
            return None;
        }
        // SDL exposes NSWindow, whereas the shared GPU ABI expects its live content NSView.
        let view: *mut objc2::runtime::AnyObject =
            unsafe { objc2::msg_send![nswindow.cast::<objc2::runtime::AnyObject>(), contentView] };
        if view.is_null() {
            return None;
        }
        out.platform = dream_host_abi::window_surface::WINDOW_PLATFORM_APPKIT;
        out.window = view.cast();
    }
    #[cfg(target_os = "windows")]
    {
        out.platform = dream_host_abi::window_surface::WINDOW_PLATFORM_WIN32;
        out.window = pointer(video::SDL_PROP_WINDOW_WIN32_HWND_POINTER);
        out.display = pointer(video::SDL_PROP_WINDOW_WIN32_INSTANCE_POINTER);
        if out.window.is_null() {
            return None;
        }
    }
    #[cfg(target_os = "linux")]
    {
        out.display = pointer(video::SDL_PROP_WINDOW_X11_DISPLAY_POINTER);
        if !out.display.is_null() {
            out.platform = dream_host_abi::window_surface::WINDOW_PLATFORM_XLIB;
            out.xlib_window = unsafe {
                properties::SDL_GetNumberProperty(
                    props,
                    video::SDL_PROP_WINDOW_X11_WINDOW_NUMBER,
                    0,
                )
            } as _;
            out.xlib_screen = unsafe {
                properties::SDL_GetNumberProperty(
                    props,
                    video::SDL_PROP_WINDOW_X11_SCREEN_NUMBER,
                    0,
                )
            } as i32;
            if out.xlib_window == 0 {
                return None;
            }
        } else {
            out.platform = dream_host_abi::window_surface::WINDOW_PLATFORM_WAYLAND;
            out.display = pointer(video::SDL_PROP_WINDOW_WAYLAND_DISPLAY_POINTER);
            out.window = pointer(video::SDL_PROP_WINDOW_WAYLAND_SURFACE_POINTER);
            if out.window.is_null() || out.display.is_null() {
                return None;
            }
        }
    }
    #[cfg(target_os = "android")]
    {
        out.platform = dream_host_abi::window_surface::WINDOW_PLATFORM_ANDROID;
        out.window = pointer(video::SDL_PROP_WINDOW_ANDROID_WINDOW_POINTER);
        if out.window.is_null() {
            return None;
        }
    }
    #[cfg(target_os = "ios")]
    {
        out.platform = dream_host_abi::window_surface::WINDOW_PLATFORM_UIKIT;
        out.window = pointer(video::SDL_PROP_WINDOW_UIKIT_WINDOW_POINTER);
        if out.window.is_null() {
            return None;
        }
    }
    if out.platform == 0 {
        return None;
    }
    Some(out)
}
