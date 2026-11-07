//! The platform-owned loop for targets whose operating system keeps the main thread
//! (`dream_abi::c_abi::PLATFORM_APP_MAIN`). SDL's main callbacks drive it: initialization runs
//! the guest entry once, every iteration runs one bounded scheduler pass, and events are only
//! recorded. SDL may deliver lifecycle events from another thread or while a pass is running, so
//! the callback never re-enters Dream; the next `windowPoll` reads the recorded events.

use sdl3::event::Event;
use sdl3::sys::events::SDL_Event;
use sdl3::sys::init::{SDL_APP_CONTINUE, SDL_APP_FAILURE, SDL_APP_SUCCESS, SDL_AppResult};
use sdl3::sys::main::{SDL_EnterAppMainCallbacks, SDL_RunApp};
use std::ffi::{c_char, c_int, c_void};
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
#[unsafe(no_mangle)]
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
    if (guest.pump)() == 0 {
        SDL_APP_SUCCESS
    } else {
        SDL_APP_CONTINUE
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
