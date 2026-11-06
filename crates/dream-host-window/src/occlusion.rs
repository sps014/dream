//! Whether the platform currently shows any part of a window. GLFW has no occlusion callback,
//! so `poll` samples this and reports edges. `None` means the platform cannot tell.

#[cfg(target_os = "macos")]
pub fn is_hidden(window: &glfw::Window) -> Option<bool> {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    /// `NSWindowOcclusionStateVisible`.
    const VISIBLE: usize = 1 << 1;
    let ns_window = window.get_cocoa_window().cast::<AnyObject>();
    if ns_window.is_null() {
        return None;
    }
    // SAFETY: GLFW owns a live NSWindow for this window on the main thread, and
    // `occlusionState` takes no arguments and returns an NSUInteger bitmask.
    let state: usize = unsafe { msg_send![&*ns_window, occlusionState] };
    Some(state & VISIBLE == 0)
}

#[cfg(target_os = "windows")]
pub fn is_hidden(window: &glfw::Window) -> Option<bool> {
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};

    let hwnd = window.get_win32_window();
    if hwnd.is_null() {
        return None;
    }
    let mut cloaked: u32 = 0;
    // SAFETY: `hwnd` is GLFW's live window handle and the buffer is a u32, the size
    // DWMWA_CLOAKED writes.
    let status = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED as u32,
            (&mut cloaked as *mut u32).cast(),
            size_of::<u32>() as u32,
        )
    };
    (status == 0).then_some(cloaked != 0)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn is_hidden(_window: &glfw::Window) -> Option<bool> {
    None
}
