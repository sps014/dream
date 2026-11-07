//! The one thread allowed to call SDL3.
//!
//! Dream's native `main` and its async scheduler run on the process main thread, so every guest
//! call outside a `Task` worker already arrives there. Workers are refused instead of marshalled:
//! AppKit aborts the process when windows are touched from another thread.

use std::sync::OnceLock;
use std::thread::ThreadId;

static OWNER: OnceLock<ThreadId> = OnceLock::new();

/// True on the process main thread (macOS, iOS, Linux), or elsewhere on the first thread that used
/// the host, which SDL3 then requires for every later call.
pub fn is_window_thread() -> bool {
    let current = std::thread::current().id();
    platform_main_thread() && *OWNER.get_or_init(|| current) == current
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn platform_main_thread() -> bool {
    unsafe { libc::pthread_main_np() != 0 }
}

#[cfg(target_os = "linux")]
fn platform_main_thread() -> bool {
    unsafe { libc::gettid() == libc::getpid() }
}

#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "linux")))]
fn platform_main_thread() -> bool {
    true
}
