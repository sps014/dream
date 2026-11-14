//! The platform-owned loop for targets whose operating system keeps the main thread
//! (`@main_loop` in `system.window`). SDL's main callbacks drive it: initialization runs
//! the guest entry once, every iteration runs one bounded scheduler pass, and events are only
//! recorded. SDL may deliver lifecycle events from another thread or while a pass is running, so
//! the callback never re-enters Dream; the next `windowPoll` reads the recorded events.

use sdl3::event::Event;
use sdl3::sys::events::SDL_Event;
use sdl3::sys::init::{SDL_APP_CONTINUE, SDL_APP_FAILURE, SDL_APP_SUCCESS, SDL_AppResult};
use sdl3::sys::main::{SDL_EnterAppMainCallbacks, SDL_RunApp};
use std::ffi::{c_char, c_int, c_void};
#[cfg(target_os = "android")]
use std::sync::Condvar;
use std::sync::{Mutex, OnceLock};

type GuestFn = extern "C" fn() -> c_int;

struct Guest {
    entry: GuestFn,
    pump: GuestFn,
}

static GUEST: OnceLock<Guest> = OnceLock::new();

/// SDL events carry raw pointers only in user events. The one user event this host posts is the
/// scheduler wake, whose data fields are null, and no recorded pointer is ever dereferenced.
struct Recorded(Event);
unsafe impl Send for Recorded {}

static INBOX: Mutex<Vec<Recorded>> = Mutex::new(Vec::new());

pub(crate) fn take_recorded() -> impl Iterator<Item = Event> {
    std::mem::take(&mut *INBOX.lock().unwrap_or_else(|poison| poison.into_inner()))
        .into_iter()
        .map(|recorded| recorded.0)
}

/// Runs the app until the guest has no outstanding work or the platform ends it.
///
/// # Safety
/// `argv` must be the process argument vector, and both functions must be the guest's
/// `dream_guest_entry` and `dream_scheduler_pump`.
pub unsafe extern "C" fn dream_window_app_main(
    argc: c_int,
    argv: *mut *mut c_char,
    entry: GuestFn,
    pump: GuestFn,
) -> c_int {
    if GUEST.set(Guest { entry, pump }).is_err() {
        return 1;
    }
    unsafe { SDL_RunApp(argc, argv, Some(enter_callbacks), std::ptr::null_mut()) }
}

unsafe extern "C" fn enter_callbacks(argc: c_int, argv: *mut *mut c_char) -> c_int {
    unsafe {
        SDL_EnterAppMainCallbacks(
            argc,
            argv,
            Some(app_init),
            Some(app_iterate),
            Some(app_event),
            Some(app_quit),
        )
    }
}

unsafe extern "C" fn app_init(
    _state: *mut *mut c_void,
    _argc: c_int,
    _argv: *mut *mut c_char,
) -> SDL_AppResult {
    let Some(guest) = GUEST.get() else {
        return SDL_APP_FAILURE;
    };
    use_bundle_directory();
    if (guest.entry)() == 0 {
        SDL_APP_CONTINUE
    } else {
        SDL_APP_FAILURE
    }
}

unsafe extern "C" fn app_iterate(_state: *mut c_void) -> SDL_AppResult {
    let Some(guest) = GUEST.get() else {
        return SDL_APP_FAILURE;
    };
    #[cfg(target_os = "android")]
    let started = std::time::Instant::now();
    service_surface_release();
    // 0 finishes the loop. 1 still has runnable work, so Android waits out the rest of the
    // display frame. 2 already waited for a timer or a foreign completion.
    let status = (guest.pump)();
    service_surface_release();
    if status == 0 {
        SDL_APP_SUCCESS
    } else {
        #[cfg(target_os = "android")]
        if status == 1 {
            pace_frame(started);
        }
        SDL_APP_CONTINUE
    }
}

/// Package assets are ordinary files under the directory the guest starts in: Android's
/// internal storage, where the activity extracted the APK assets, or the iOS bundle.
fn use_bundle_directory() {
    #[cfg(target_os = "android")]
    unsafe {
        let path = sdl3::sys::system::SDL_GetAndroidInternalStoragePath();
        if !path.is_null() {
            let directory = std::ffi::CStr::from_ptr(path).to_string_lossy();
            let _ = std::env::set_current_dir(directory.as_ref());
        }
    }
    #[cfg(target_os = "ios")]
    if let Some(directory) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
    {
        let _ = std::env::set_current_dir(directory);
    }
}

#[cfg(target_os = "android")]
fn pace_frame(started: std::time::Instant) {
    let Some(frame) = crate::host::refresh_interval() else {
        return;
    };
    let spent = started.elapsed();
    if let Some(rest) = frame.checked_sub(spent) {
        crate::host::park(i64::try_from(rest.as_nanos()).unwrap_or(i64::MAX));
    }
}

/// The Java UI thread sets the request in `surfaceDestroyed` and waits, so the swapchains are
/// gone before Android frees the native window they were created from.
#[cfg(target_os = "android")]
struct SurfaceRelease {
    requested: bool,
    done: bool,
}

#[cfg(target_os = "android")]
static SURFACE_RELEASE: (Mutex<SurfaceRelease>, Condvar) = (
    Mutex::new(SurfaceRelease {
        requested: false,
        done: true,
    }),
    Condvar::new(),
);

fn service_surface_release() {
    #[cfg(target_os = "android")]
    {
        let requested = {
            let mut gate = SURFACE_RELEASE
                .0
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if !gate.requested {
                return;
            }
            gate.requested = false;
            true
        };
        if requested {
            release_gpu_surfaces();
            let mut gate = SURFACE_RELEASE
                .0
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            gate.done = true;
            SURFACE_RELEASE.1.notify_one();
        }
    }
}

#[cfg(target_os = "android")]
fn release_gpu_surfaces() {
    unsafe extern "C" {
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }
    let symbol = unsafe {
        dlsym(
            usize::MAX as *mut c_void,
            c"dream_gpu_release_window_surfaces".as_ptr(),
        )
    };
    if !symbol.is_null() {
        let release: extern "C" fn() = unsafe { std::mem::transmute(symbol) };
        release();
    }
}

/// Called on Android's UI thread from `surfaceDestroyed`, before SDL releases the native window.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn Java_org_dream_runtime_Surfaces_release() {
    {
        let mut gate = SURFACE_RELEASE
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        gate.requested = true;
        gate.done = false;
    }
    crate::host::wake();
    let mut gate = SURFACE_RELEASE
        .0
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    while !gate.done {
        let now = std::time::Instant::now();
        if now >= deadline {
            break;
        }
        let (next, timeout) = SURFACE_RELEASE
            .1
            .wait_timeout(gate, deadline.saturating_duration_since(now))
            .unwrap_or_else(|poison| poison.into_inner());
        gate = next;
        if timeout.timed_out() {
            break;
        }
    }
}

unsafe extern "C" fn app_event(_state: *mut c_void, event: *mut SDL_Event) -> SDL_AppResult {
    if let Some(event) = unsafe { event.as_ref() } {
        INBOX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(Recorded(Event::from_ll(*event)));
    }
    SDL_APP_CONTINUE
}

unsafe extern "C" fn app_quit(_state: *mut c_void, _result: SDL_AppResult) {
    crate::host::shutdown();
}

#[cfg(target_os = "android")]
unsafe extern "C" {
    /// SDL's `JNI_OnLoad`, renamed by the Android `CFLAGS` in `.cargo/config.toml`.
    fn dream_sdl_jni_onload(vm: *mut c_void, reserved: *mut c_void) -> c_int;
}

/// A cdylib exports only Rust-defined symbols, so SDL's JNI registration is re-exported here for
/// `System.loadLibrary`.
#[cfg(target_os = "android")]
#[allow(non_snake_case)] // The JNI specification fixes this name.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn JNI_OnLoad(vm: *mut c_void, reserved: *mut c_void) -> c_int {
    unsafe { dream_sdl_jni_onload(vm, reserved) }
}
