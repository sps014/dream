use crate::host;
use dream_host_abi::{DreamPtr, alloc_f32s, alloc_string, read_string};

/// # Safety
/// The title must be a readable guest string for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn windowOpen(title: DreamPtr, width: i32, height: i32, flags: i32) -> i32 {
    let title = unsafe { read_string(title) };
    match host::open(&title, width, height, flags) {
        Ok(id) => id,
        Err(error) => {
            host::record_error(error);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn windowError() -> DreamPtr {
    alloc_string(&host::last_error_text())
}

#[unsafe(no_mangle)]
pub extern "C" fn windowClose(window: i32) {
    host::close(window);
}

#[unsafe(no_mangle)]
pub extern "C" fn windowPoll(window: i32) -> DreamPtr {
    alloc_f32s(&host::poll(window))
}

#[unsafe(no_mangle)]
pub extern "C" fn windowQuery(window: i32) -> DreamPtr {
    alloc_f32s(&host::query(window))
}

/// # Safety
/// The title must be a readable guest string for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn windowSetTitle(window: i32, title: DreamPtr) {
    let title = unsafe { read_string(title) };
    host::set_title(window, &title);
}

#[unsafe(no_mangle)]
pub extern "C" fn windowSetSize(window: i32, width: i32, height: i32) {
    host::set_size(window, width, height);
}

#[unsafe(no_mangle)]
pub extern "C" fn windowSetFullscreen(window: i32, enabled: i32) {
    host::set_fullscreen(window, enabled != 0);
}

#[unsafe(no_mangle)]
pub extern "C" fn windowSetCursorMode(window: i32, mode: i32) {
    host::set_cursor_mode(window, mode);
}

#[unsafe(no_mangle)]
pub extern "C" fn windowSetCursorIcon(window: i32, icon: i32) {
    host::set_cursor_icon(window, icon);
}

// GLFW exposes no force-feedback API, so native rumble is reported as unsupported.
#[unsafe(no_mangle)]
pub extern "C" fn windowGamepadRumble(_slot: i32, _low: f32, _high: f32, _ms: i32) -> i32 {
    0
}
