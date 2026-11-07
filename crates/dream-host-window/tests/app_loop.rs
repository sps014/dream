//! Drives `dream_window_app_main` on the process main thread (`harness = false`) the way an
//! Android or iOS guest does: SDL's main callbacks call the entry once, then the pump every
//! iteration, and events reach `windowPoll` through the recorded inbox. The loop ends SDL, so
//! it needs its own process.

use dream_host_window::{app::dream_window_app_main, host, wire::kind};
use std::ffi::c_int;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

static ENTRIES: AtomicUsize = AtomicUsize::new(0);
static PUMPS: AtomicUsize = AtomicUsize::new(0);
static WINDOW: AtomicI32 = AtomicI32::new(0);

extern "C" fn entry() -> c_int {
    ENTRIES.fetch_add(1, Ordering::SeqCst);
    let id = host::open("app", 320, 200, 1 | 4).expect("dummy window");
    host::poll(id);
    WINDOW.store(id, Ordering::SeqCst);
    0
}

fn records(values: &[f32]) -> Vec<(u8, Vec<f32>)> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 1 < values.len() {
        let length = values[at + 1] as usize;
        out.push((values[at] as u8, values[at + 2..at + 2 + length].to_vec()));
        at += 2 + length;
    }
    out
}

extern "C" fn pump() -> c_int {
    let frame = PUMPS.fetch_add(1, Ordering::SeqCst);
    let id = WINDOW.load(Ordering::SeqCst);
    match frame {
        0 => {
            host::set_size(id, 640, 480);
            1
        }
        1 => {
            let events = records(&host::poll(id));
            assert!(
                events.contains(&(kind::RESIZED, vec![640.0, 480.0])),
                "events SDL dispatched to the app callback reach the next poll: {events:?}"
            );
            1
        }
        _ => {
            host::close(id);
            0
        }
    }
}

fn main() {
    // No SDL or other threads exist yet, so configuring the process driver is safe.
    unsafe { std::env::set_var("SDL_VIDEO_DRIVER", "dummy") };
    let mut name = *b"app\0";
    let mut argv = [name.as_mut_ptr().cast(), std::ptr::null_mut()];
    let status = unsafe { dream_window_app_main(1, argv.as_mut_ptr(), entry, pump) };
    assert_eq!(status, 0, "an idle pump ends the app successfully");
    assert_eq!(ENTRIES.load(Ordering::SeqCst), 1);
    assert_eq!(
        PUMPS.load(Ordering::SeqCst),
        3,
        "the loop stops at the first idle pump"
    );
    assert_eq!(
        unsafe { dream_window_app_main(1, argv.as_mut_ptr(), entry, pump) },
        1,
        "a process runs one platform loop"
    );
    println!("dream-host-window app-loop checks passed");
}
