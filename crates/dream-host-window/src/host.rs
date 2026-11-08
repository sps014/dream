//! Thread-local window ownership. Platform events never call into guest code.

use crate::{gamepads::Pads, input::Input, metrics::Metrics, thread::is_window_thread};
use dream_host_abi::{SchedulerPark, window_surface::WindowSurface};
use sdl3::{EventPump, Sdl, VideoSubsystem, event::Event, joystick::JoystickId, video::Window};
use std::{cell::RefCell, collections::BTreeMap, mem::ManuallyDrop};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostError {
    pub code: &'static str,
    pub message: String,
}

impl HostError {
    fn new(code: &'static str, message: impl ToString) -> Self {
        Self {
            code,
            message: message.to_string(),
        }
    }
    fn wrong_thread() -> Self {
        Self::new(
            "ETHREAD",
            "windows can only be used from the program's main thread, not from a Task",
        )
    }
}

pub(crate) struct Entry {
    // Fields drop in order: the Metal view is a subview of the window.
    #[cfg(target_os = "ios")]
    pub metal: crate::surface::MetalView,
    pub window: Window,
    pub input: Input,
    pub metrics: Metrics,
    pub pads: [Option<JoystickId>; 16],
    pub vsync: bool,
    pub cursor_mode: i32,
    pub cursor_icon: usize,
}

pub(crate) struct Host {
    // SDL resources must be dropped before the subsystems they belong to.
    pub windows: BTreeMap<i32, Entry>,
    pub native_ids: BTreeMap<u32, i32>,
    pub cursors: [Option<sdl3::mouse::Cursor>; 10],
    pub parked: Vec<Event>,
    pub pads: Pads,
    pub pump: EventPump,
    pub video: VideoSubsystem,
    pub sdl: Sdl,
    next_id: i32,
    pub background: bool,
    any_focused: bool,
}

thread_local! {
    // No thread-exit destructor: on Windows it would run SDL's teardown inside `ExitProcess`
    // under the loader lock. `shutdown` releases the host explicitly where a platform loop ends.
    static HOST: ManuallyDrop<RefCell<Option<Host>>> = const { ManuallyDrop::new(RefCell::new(None)) };
    static LAST_ERROR: RefCell<Option<HostError>> = const { RefCell::new(None) };
}

/// Initializes the platform video driver once on the window thread.
pub fn initialize() -> Result<(), HostError> {
    if !is_window_thread() {
        return Err(HostError::wrong_thread());
    }
    HOST.with(|cell| {
        let mut host = cell.borrow_mut();
        if host.is_some() {
            return Ok(());
        }
        sdl3::hint::set("SDL_TOUCH_MOUSE_EVENTS", "0");
        sdl3::hint::set("SDL_MOUSE_TOUCH_EVENTS", "0");
        // The pinned GPU surface implementation supports Xlib, including XWayland.
        #[cfg(target_os = "linux")]
        sdl3::hint::set("SDL_VIDEO_DRIVER", "x11");
        let failure = |error| HostError::new("ENODISPLAY", error);
        let sdl = sdl3::init().map_err(failure)?;
        let video = sdl.video().map_err(failure)?;
        let pads = Pads::new(sdl.gamepad().map_err(failure)?);
        let pump = sdl.event_pump().map_err(failure)?;
        crate::pump::initialize_wake(&sdl).map_err(failure)?;
        *host = Some(Host {
            windows: BTreeMap::new(),
            native_ids: BTreeMap::new(),
            cursors: std::array::from_fn(|_| None),
            parked: Vec::new(),
            pads,
            pump,
            video,
            sdl,
            next_id: 1,
            background: false,
            any_focused: true,
        });
        Ok(())
    })
}

pub(crate) fn with_host<R>(f: impl FnOnce(&mut Host) -> R) -> Option<R> {
    if !is_window_thread() {
        return None;
    }
    HOST.with(|cell| cell.borrow_mut().as_mut().map(f))
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
    if width <= 0 || height <= 0 || title.contains('\0') {
        return Err(HostError::new(
            "EINVAL",
            "window dimensions must be positive and the title cannot contain NUL",
        ));
    }
    initialize()?;
    with_host(|host| {
        let next = host
            .next_id
            .checked_add(1)
            .ok_or_else(|| HostError::new("ELIMIT", "window handle limit reached"))?;
        let mut builder = host.video.window(title, width as u32, height as u32);
        builder.high_pixel_density();
        if flags & 1 != 0 {
            builder.resizable();
        }
        if flags & 2 != 0 {
            builder.fullscreen();
        }
        #[cfg(target_os = "ios")]
        builder.metal_view();
        let mut window = builder
            .build()
            .map_err(|error| HostError::new("EWINDOW", error))?;
        let initial = Metrics::read(&window);
        if flags & 2 == 0 && initial.coordinates != 1.0 {
            window
                .set_size(
                    (width as f32 * initial.coordinates).round() as u32,
                    (height as f32 * initial.coordinates).round() as u32,
                )
                .map_err(|error| HostError::new("EWINDOW", error))?;
        }
        // Text editing and mobile IME are deferred; desktop text events remain available.
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        host.video.text_input().start(&window);
        let metrics = Metrics::read(&window);
        let mut input = Input::default();
        input.focused = window.has_input_focus();
        if metrics.insets != [0.0; 4] {
            input
                .wire
                .push(crate::wire::kind::SAFE_AREA_CHANGED, &metrics.insets);
        }
        #[cfg(target_os = "ios")]
        let metal = crate::surface::MetalView::create(&window)
            .ok_or_else(|| HostError::new("EWINDOW", sdl3::get_error()))?;
        let id = host.next_id;
        host.next_id = next;
        host.native_ids.insert(window.id(), id);
        host.windows.insert(
            id,
            Entry {
                #[cfg(target_os = "ios")]
                metal,
                window,
                input,
                metrics,
                pads: [None; 16],
                vsync: flags & 4 != 0,
                cursor_mode: 0,
                cursor_icon: 0,
            },
        );
        dream_host_abi::set_scheduler_park(Some(SchedulerPark {
            park: park_hook,
            wake: wake_hook,
        }));
        Ok(id)
    })
    .unwrap_or_else(|| Err(HostError::wrong_thread()))
}

pub fn close(id: i32) {
    with_host(|host| {
        if let Some(entry) = host.windows.remove(&id) {
            host.native_ids.remove(&entry.window.id());
        }
        if host.windows.is_empty() {
            dream_host_abi::set_scheduler_park(None);
            host.sdl.mouse().show_cursor(true);
        }
        report_activity(host);
    });
}

/// Releases every window and subsystem while SDL is still initialized, before the platform loop
/// shuts SDL down.
pub(crate) fn shutdown() {
    if !is_window_thread() {
        return;
    }
    dream_host_abi::set_scheduler_park(None);
    HOST.with(|cell| *cell.borrow_mut() = None);
}

/// The display period of a visible vsync window, used to pace Android frames that returned early.
#[cfg(target_os = "android")]
pub(crate) fn refresh_interval() -> Option<std::time::Duration> {
    with_host(|host| {
        let pacing = host
            .windows
            .values()
            .any(|entry| entry.vsync && !entry.input.hidden && !host.background);
        if !pacing {
            return None;
        }
        let mode = unsafe {
            sdl3::sys::video::SDL_GetCurrentDisplayMode(sdl3::sys::video::SDL_GetPrimaryDisplay())
        };
        if mode.is_null() {
            return None;
        }
        let hz = f64::from(unsafe { (*mode).refresh_rate });
        (hz >= 1.0).then(|| std::time::Duration::from_nanos((1_000_000_000.0 / hz) as u64))
    })
    .flatten()
}

/// Waits for platform events, preserving the event consumed by the wait for the next poll.
pub fn park(timeout_ns: i64) {
    with_host(|host| {
        if !host.parked.is_empty() {
            return;
        }
        let ms = if timeout_ns < 0 {
            u32::MAX
        } else {
            (timeout_ns.saturating_add(999_999) / 1_000_000).min(i64::from(i32::MAX)) as u32
        };
        if let Some(event) = host.pump.wait_event_timeout_ms(ms) {
            host.parked.push(event);
        }
    });
}

/// Makes a concurrent or upcoming `park` return. Safe from any thread after initialization.
pub fn wake() {
    crate::pump::wake();
}
unsafe extern "C" fn park_hook(timeout_ns: i64, _context: *mut u8) {
    park(timeout_ns);
}
unsafe extern "C" fn wake_hook(_context: *mut u8) {
    wake();
}

pub fn poll(id: i32) -> Vec<f32> {
    with_host(|host| {
        crate::pump::drain(host);
        host.pads.update();
        report_activity(host);
        let Some(entry) = host.windows.get_mut(&id) else {
            return Vec::new();
        };
        host.pads.sample(&mut entry.pads, &mut entry.input.wire);
        std::mem::take(&mut entry.input.wire).into_values()
    })
    .unwrap_or_default()
}

fn report_activity(host: &mut Host) {
    let any = host.windows.values().any(|entry| entry.input.focused) && !host.background;
    if any != host.any_focused {
        host.any_focused = any;
        dream_host_abi::report_app_activity(any);
    }
}

/// `[width, height, pixel_width, pixel_height, scale, fullscreen, focused]`; empty when unknown.
pub fn query(id: i32) -> Vec<f32> {
    with_host(|host| {
        let Some(entry) = host.windows.get(&id) else {
            return Vec::new();
        };
        let metrics = Metrics::read(&entry.window);
        vec![
            metrics.width,
            metrics.height,
            metrics.pixels.0 as f32,
            metrics.pixels.1 as f32,
            metrics.scale,
            u8::from(entry.window.fullscreen_state() != sdl3::video::FullscreenType::Off) as f32,
            u8::from(entry.window.has_input_focus() && !host.background) as f32,
        ]
    })
    .unwrap_or_default()
}

pub fn set_title(id: i32, title: &str) {
    with_host(|host| {
        if let Some(entry) = host.windows.get_mut(&id) {
            let _ = entry.window.set_title(title);
        }
    });
}
pub fn set_size(id: i32, width: i32, height: i32) {
    if width <= 0 || height <= 0 {
        return;
    }
    with_host(|host| {
        if let Some(entry) = host.windows.get_mut(&id) {
            let scale = Metrics::read(&entry.window).coordinates;
            let _ = entry.window.set_size(
                (width as f32 * scale).round() as u32,
                (height as f32 * scale).round() as u32,
            );
        }
    });
}
pub fn set_fullscreen(id: i32, enabled: bool) {
    with_host(|host| {
        if let Some(entry) = host.windows.get_mut(&id) {
            let _ = entry.window.set_fullscreen(enabled);
        }
    });
}
pub fn set_cursor_mode(id: i32, mode: i32) {
    with_host(|host| {
        if let Some(entry) = host.windows.get_mut(&id) {
            entry.cursor_mode = mode;
            entry.input.captured = mode == 2;
            host.sdl
                .mouse()
                .set_relative_mouse_mode(&entry.window, mode == 2);
        }
        crate::pump::apply_cursor(host, id);
    });
}
pub fn set_cursor_icon(id: i32, icon: i32) {
    with_host(|host| {
        if let Some(entry) = host.windows.get_mut(&id) {
            entry.cursor_icon = usize::try_from(icon).ok().filter(|i| *i < 10).unwrap_or(0);
        }
        crate::pump::apply_cursor(host, id);
    });
}
pub fn rumble(slot: i32, low: f32, high: f32, ms: i32) -> bool {
    with_host(|host| host.pads.rumble(slot, low, high, ms)).unwrap_or(false)
}
pub fn surface(id: i32) -> Option<WindowSurface> {
    with_host(|host| {
        let entry = host.windows.get(&id)?;
        if host.background || entry.input.hidden {
            return None;
        }
        crate::surface::native(entry)
    })
    .flatten()
}
