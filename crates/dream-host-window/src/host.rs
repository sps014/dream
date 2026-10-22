//! Window ownership and the event pump.
//!
//! All state lives in a thread-local on the window thread. `poll` runs `glfwPollEvents`, whose
//! callbacks fill each window's GLFW receiver queue; the same call drains that queue into the
//! wire, so no event is delivered from any other thread.

use crate::thread::is_window_thread;
use crate::wire::{self, Wire, kind};
use dream_host_abi::SchedulerPark;
use dream_host_abi::window_surface::{self as surface, WindowSurface};
use glfw::{
    ClientApiHint, Context, CursorMode, Glfw, GlfwReceiver, InitHint, JoystickId, PWindow,
    Platform, WindowEvent, WindowHint, WindowMode, ffi,
};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use std::cell::RefCell;
use std::collections::BTreeMap;

const FLAG_RESIZABLE: i32 = 1;
const FLAG_FULLSCREEN: i32 = 2;
const FLAG_VSYNC: i32 = 4;
const GAMEPAD_SLOTS: i32 = 16;

/// `CursorIcon` order; GLFW 3.4 shape ids.
const CURSOR_SHAPES: [i32; 10] = [
    ffi::GLFW_ARROW_CURSOR,
    ffi::GLFW_IBEAM_CURSOR,
    ffi::GLFW_CROSSHAIR_CURSOR,
    ffi::GLFW_POINTING_HAND_CURSOR,
    ffi::GLFW_RESIZE_EW_CURSOR,
    ffi::GLFW_RESIZE_NS_CURSOR,
    ffi::GLFW_RESIZE_NWSE_CURSOR,
    ffi::GLFW_RESIZE_NESW_CURSOR,
    ffi::GLFW_RESIZE_ALL_CURSOR,
    ffi::GLFW_NOT_ALLOWED_CURSOR,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostError {
    pub code: &'static str,
    pub message: String,
}

impl HostError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn wrong_thread() -> Self {
        Self::new(
            "ETHREAD",
            "windows can only be used from the program's main thread, not from a Task",
        )
    }
}

struct Entry {
    window: PWindow,
    events: GlfwReceiver<(f64, WindowEvent)>,
    pads: u32,
    vsync: bool,
    /// Windowed position and size to restore when leaving fullscreen.
    restore: (i32, i32, i32, i32),
}

struct Host {
    glfw: Glfw,
    windows: BTreeMap<i32, Entry>,
    next_id: i32,
    cursors: [*mut ffi::GLFWcursor; CURSOR_SHAPES.len()],
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
    static LAST_ERROR: RefCell<Option<HostError>> = const { RefCell::new(None) };
    static GLFW_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Starts GLFW on `platform`. `open` does this with `Platform::Any`; tests pick `Platform::Null`
/// to run without a display. Later calls keep the platform chosen first.
pub fn initialize(platform: Platform) -> Result<(), HostError> {
    if !is_window_thread() {
        return Err(HostError::wrong_thread());
    }
    HOST.with_borrow_mut(|host| {
        if host.is_some() {
            return Ok(());
        }
        glfw::init_hint(InitHint::Platform(platform));
        glfw::init_hint(InitHint::CocoaChdirResources(false));
        let glfw = glfw::init(|_, description: String| {
            GLFW_ERROR.with_borrow_mut(|last| *last = description);
        })
        .map_err(|_| {
            HostError::new(
                "ENODISPLAY",
                format!("no display is available: {}", take_glfw_error()),
            )
        })?;
        *host = Some(Host {
            glfw,
            windows: BTreeMap::new(),
            next_id: 1,
            cursors: [std::ptr::null_mut(); CURSOR_SHAPES.len()],
        });
        Ok(())
    })
}

fn take_glfw_error() -> String {
    let message = GLFW_ERROR.with_borrow_mut(std::mem::take);
    if message.is_empty() {
        "the platform window system did not start".into()
    } else {
        message
    }
}

fn with_host<R>(f: impl FnOnce(&mut Host) -> R) -> Option<R> {
    if !is_window_thread() {
        return None;
    }
    HOST.with_borrow_mut(|host| host.as_mut().map(f))
}

pub fn record_error(error: HostError) {
    LAST_ERROR.with_borrow_mut(|last| *last = Some(error));
}

/// `CODE\nmessage` for the most recent failed `open`, which `Window.last_error` splits.
pub fn last_error_text() -> String {
    LAST_ERROR.with_borrow(|last| match last {
        Some(error) => format!("{}\n{}", error.code, error.message),
        None => "EWINDOW\nwindow creation failed".into(),
    })
}

pub fn open(title: &str, width: i32, height: i32, flags: i32) -> Result<i32, HostError> {
    if width <= 0 || height <= 0 {
        return Err(HostError::new(
            "EINVAL",
            "window width and height must be positive",
        ));
    }
    initialize(Platform::Any)?;
    with_host(|host| {
        let Host {
            glfw,
            windows,
            next_id,
            ..
        } = host;
        glfw.default_window_hints();
        glfw.window_hint(WindowHint::ClientApi(ClientApiHint::NoApi));
        glfw.window_hint(WindowHint::Resizable(flags & FLAG_RESIZABLE != 0));
        glfw.window_hint(WindowHint::ScaleToMonitor(true));
        let created = if flags & FLAG_FULLSCREEN != 0 {
            glfw.with_primary_monitor(|glfw, monitor| match monitor {
                Some(monitor) => {
                    let (w, h) = monitor
                        .get_video_mode()
                        .map(|mode| (mode.width, mode.height))
                        .unwrap_or((width as u32, height as u32));
                    glfw.create_window(w, h, title, WindowMode::FullScreen(monitor))
                }
                None => {
                    glfw.create_window(width as u32, height as u32, title, WindowMode::Windowed)
                }
            })
        } else {
            glfw.create_window(width as u32, height as u32, title, WindowMode::Windowed)
        };
        let Some((mut window, events)) = created else {
            return Err(HostError::new(
                "EWINDOW",
                format!(
                    "the platform refused to create a window: {}",
                    take_glfw_error()
                ),
            ));
        };
        window.set_all_polling(true);
        unsafe { ffi::glfwSetInputMode(window.window_ptr(), ffi::GLFW_LOCK_KEY_MODS, 1) };
        if windows.is_empty() {
            dream_host_abi::set_scheduler_park(Some(SchedulerPark {
                park: park_hook,
                wake: wake_hook,
            }));
        }
        let id = *next_id;
        *next_id += 1;
        windows.insert(
            id,
            Entry {
                window,
                events,
                pads: 0,
                vsync: flags & FLAG_VSYNC != 0,
                restore: (100, 100, width, height),
            },
        );
        Ok(id)
    })
    .unwrap_or_else(|| Err(HostError::wrong_thread()))
}

pub fn close(id: i32) {
    with_host(|host| {
        if host.windows.remove(&id).is_some() && host.windows.is_empty() {
            dream_host_abi::set_scheduler_park(None);
        }
    });
}

/// Waits for platform events for at most `timeout_ns` (indefinitely when negative), running
/// GLFW's callbacks so their events wait in each window's queue for the next `poll`. Returns
/// early when `wake` is called from any thread.
pub fn park(timeout_ns: i64) {
    if !is_window_thread() || HOST.with_borrow(Option::is_none) {
        return;
    }
    // Direct FFI: the scheduler parks between guest calls, and GLFW callbacks only touch the
    // receivers' channels, never the `HOST` cell.
    if timeout_ns < 0 {
        unsafe { ffi::glfwWaitEvents() };
    } else {
        unsafe { ffi::glfwWaitEventsTimeout(timeout_ns as f64 / 1e9) };
    }
}

/// Makes a concurrent or upcoming `park` return. Safe from any thread once GLFW is initialized.
pub fn wake() {
    unsafe { ffi::glfwPostEmptyEvent() };
}

unsafe extern "C" fn park_hook(timeout_ns: i64, _context: *mut u8) {
    park(timeout_ns);
}

unsafe extern "C" fn wake_hook(_context: *mut u8) {
    wake();
}

/// Pumps the platform queue and returns this window's events plus gamepad changes and samples.
pub fn poll(id: i32) -> Vec<f32> {
    with_host(|host| {
        host.glfw.poll_events();
        let Some(entry) = host.windows.get_mut(&id) else {
            return Vec::new();
        };
        let mut out = Wire::default();
        let scale = coord_scale(&entry.window);
        let key_name = |key, scancode| glfw::get_key_name(Some(key), Some(scancode));
        for (_, event) in glfw::flush_messages(&entry.events) {
            wire::encode_event(&event, scale, &key_name, &mut out);
        }
        poll_gamepads(&host.glfw, entry, &mut out);
        out.into_values()
    })
    .unwrap_or_default()
}

fn poll_gamepads(glfw: &Glfw, entry: &mut Entry, out: &mut Wire) {
    for slot in 0..GAMEPAD_SLOTS {
        let Some(id) = JoystickId::from_i32(slot) else {
            continue;
        };
        let joystick = glfw.get_joystick(id);
        let bit = 1u32 << slot;
        if joystick.is_present() && joystick.is_gamepad() {
            if entry.pads & bit == 0 {
                entry.pads |= bit;
                let name = joystick.get_gamepad_name().unwrap_or_default();
                out.push_text(kind::GAMEPAD_CONNECTED, &[slot as f32], &name);
            }
            if let Some(state) = joystick.get_gamepad_state() {
                let mut buttons = 0u32;
                for index in 0..=ffi::GLFW_GAMEPAD_BUTTON_LAST {
                    if let Some(button) = glfw::GamepadButton::from_i32(index)
                        && state.get_button_state(button) != glfw::Action::Release
                    {
                        buttons |= 1 << index;
                    }
                }
                let mut axes = [0.0f32; 6];
                for (index, value) in axes.iter_mut().enumerate() {
                    if let Some(axis) = glfw::GamepadAxis::from_i32(index as i32) {
                        *value = state.get_axis(axis);
                    }
                }
                wire::encode_gamepad_sample(slot, buttons, axes, out);
            }
        } else if entry.pads & bit != 0 {
            entry.pads &= !bit;
            out.push(kind::GAMEPAD_DISCONNECTED, &[slot as f32]);
        }
    }
}

/// GLFW screen coordinates per logical pixel. macOS (and Wayland) report logical coordinates
/// with a larger framebuffer; Win32 and X11 report pixels, scaled by the monitor content scale.
fn coord_scale(window: &glfw::Window) -> f64 {
    let (width, _) = window.get_size();
    let (pixels, _) = window.get_framebuffer_size();
    let (content, _) = window.get_content_scale();
    if width > 0 && width == pixels && content > 0.0 {
        f64::from(content)
    } else {
        1.0
    }
}

/// `[width, height, pixel_width, pixel_height, scale, fullscreen, focused]`; empty when unknown.
pub fn query(id: i32) -> Vec<f32> {
    with_host(|host| {
        let Some(entry) = host.windows.get(&id) else {
            return Vec::new();
        };
        let window = &entry.window;
        let scale = coord_scale(window);
        let (width, height) = window.get_size();
        let (pixels_w, pixels_h) = window.get_framebuffer_size();
        let (content, _) = window.get_content_scale();
        let fullscreen = unsafe { !ffi::glfwGetWindowMonitor(window.window_ptr()).is_null() };
        let focused =
            unsafe { ffi::glfwGetWindowAttrib(window.window_ptr(), ffi::GLFW_FOCUSED) != 0 };
        vec![
            (f64::from(width) / scale) as f32,
            (f64::from(height) / scale) as f32,
            pixels_w as f32,
            pixels_h as f32,
            content,
            f32::from(u8::from(fullscreen)),
            f32::from(u8::from(focused)),
        ]
    })
    .unwrap_or_default()
}

pub fn set_title(id: i32, title: &str) {
    with_host(|host| {
        if let Some(entry) = host.windows.get_mut(&id) {
            entry.window.set_title(title);
        }
    });
}

pub fn set_size(id: i32, width: i32, height: i32) {
    if width <= 0 || height <= 0 {
        return;
    }
    with_host(|host| {
        if let Some(entry) = host.windows.get_mut(&id) {
            let scale = coord_scale(&entry.window);
            entry.window.set_size(
                (f64::from(width) * scale).round() as i32,
                (f64::from(height) * scale).round() as i32,
            );
        }
    });
}

pub fn set_fullscreen(id: i32, enabled: bool) {
    with_host(|host| {
        let Host { glfw, windows, .. } = host;
        let Some(entry) = windows.get_mut(&id) else {
            return;
        };
        let is_fullscreen =
            unsafe { !ffi::glfwGetWindowMonitor(entry.window.window_ptr()).is_null() };
        if enabled == is_fullscreen {
            return;
        }
        if enabled {
            let (x, y) = entry.window.get_pos();
            let (w, h) = entry.window.get_size();
            entry.restore = (x, y, w, h);
            glfw.with_primary_monitor(|_, monitor| {
                let Some(monitor) = monitor else {
                    return;
                };
                let Some(mode) = monitor.get_video_mode() else {
                    return;
                };
                entry.window.set_monitor(
                    WindowMode::FullScreen(monitor),
                    0,
                    0,
                    mode.width,
                    mode.height,
                    Some(mode.refresh_rate),
                );
            });
        } else {
            let (x, y, w, h) = entry.restore;
            entry.window.set_monitor(
                WindowMode::Windowed,
                x,
                y,
                w.max(1) as u32,
                h.max(1) as u32,
                None,
            );
        }
    });
}

pub fn set_cursor_mode(id: i32, mode: i32) {
    with_host(|host| {
        let Some(entry) = host.windows.get_mut(&id) else {
            return;
        };
        let captured = mode == 2;
        entry.window.set_cursor_mode(match mode {
            1 => CursorMode::Hidden,
            2 => CursorMode::Disabled,
            _ => CursorMode::Normal,
        });
        if unsafe { ffi::glfwRawMouseMotionSupported() } != 0 {
            let raw = i32::from(captured);
            unsafe {
                ffi::glfwSetInputMode(entry.window.window_ptr(), ffi::GLFW_RAW_MOUSE_MOTION, raw)
            };
        }
    });
}

pub fn set_cursor_icon(id: i32, icon: i32) {
    with_host(|host| {
        let Some(entry) = host.windows.get(&id) else {
            return;
        };
        let index = usize::try_from(icon)
            .unwrap_or(0)
            .min(CURSOR_SHAPES.len() - 1);
        if host.cursors[index].is_null() && index != 0 {
            host.cursors[index] = unsafe { ffi::glfwCreateStandardCursor(CURSOR_SHAPES[index]) };
        }
        // A null cursor restores the platform arrow, which is also the fallback for shapes the
        // platform lacks.
        unsafe { ffi::glfwSetCursor(entry.window.window_ptr(), host.cursors[index]) };
    });
}

/// The raw handles a GPU host needs to create a surface for `id`.
pub fn surface(id: i32) -> Option<WindowSurface> {
    with_host(|host| {
        if host.glfw.get_platform() == Platform::Null {
            return None;
        }
        let entry = host.windows.get(&id)?;
        let window = entry.window.window_handle().ok()?.as_raw();
        let display = entry.window.display_handle().ok()?.as_raw();
        let (pixel_width, pixel_height) = entry.window.get_framebuffer_size();
        let mut out = WindowSurface {
            vsync: u32::from(entry.vsync),
            pixel_width: pixel_width.max(0) as u32,
            pixel_height: pixel_height.max(0) as u32,
            scale: entry.window.get_content_scale().0,
            ..WindowSurface::default()
        };
        match (window, display) {
            (RawWindowHandle::AppKit(handle), _) => {
                out.platform = surface::WINDOW_PLATFORM_APPKIT;
                out.window = handle.ns_view.as_ptr();
            }
            (RawWindowHandle::Win32(handle), _) => {
                out.platform = surface::WINDOW_PLATFORM_WIN32;
                out.window = handle.hwnd.get() as *mut _;
                out.display = handle.hinstance.map_or(0, |h| h.get()) as *mut _;
            }
            (RawWindowHandle::Xlib(handle), RawDisplayHandle::Xlib(display)) => {
                out.platform = surface::WINDOW_PLATFORM_XLIB;
                out.xlib_window = handle.window;
                out.xlib_screen = display.screen;
                out.display = display.display.map_or(std::ptr::null_mut(), |d| d.as_ptr());
            }
            (RawWindowHandle::Wayland(handle), RawDisplayHandle::Wayland(display)) => {
                out.platform = surface::WINDOW_PLATFORM_WAYLAND;
                out.window = handle.surface.as_ptr();
                out.display = display.display.as_ptr();
            }
            _ => return None,
        }
        Some(out)
    })
    .flatten()
}
