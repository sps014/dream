//! A monotonic clock that keeps counting while the computer sleeps, for `on_wake`'s
//! `slept_ms`. `std::time::Instant` stops during sleep on macOS, so it cannot measure the gap.

use std::sync::atomic::{AtomicI64, Ordering};

static SLEPT_AT: AtomicI64 = AtomicI64::new(-1);

#[cfg(unix)]
fn now_ms() -> Option<i64> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // macOS CLOCK_MONOTONIC counts time asleep; on Linux only CLOCK_BOOTTIME does.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let clock = libc::CLOCK_BOOTTIME;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let clock = libc::CLOCK_MONOTONIC;
    let status = unsafe { libc::clock_gettime(clock, &mut time) };
    (status == 0).then(|| time.tv_sec * 1000 + time.tv_nsec / 1_000_000)
}

#[cfg(windows)]
fn now_ms() -> Option<i64> {
    Some(unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() } as i64)
}

#[cfg(not(any(unix, windows)))]
fn now_ms() -> Option<i64> {
    None
}

pub fn mark_sleep() {
    SLEPT_AT.store(now_ms().unwrap_or(-1), Ordering::SeqCst);
}

/// Milliseconds since `mark_sleep`, or -1 when unknown.
pub fn slept_ms() -> i32 {
    let start = SLEPT_AT.swap(-1, Ordering::SeqCst);
    match now_ms() {
        Some(now) if start >= 0 && now >= start => (now - start).min(i32::MAX as i64) as i32,
        _ => -1,
    }
}
