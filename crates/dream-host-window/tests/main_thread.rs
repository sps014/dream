//! Runs on the process main thread (`harness = false`), the only thread SDL3 may use.
//!
//! SDL3's dummy driver needs no display, so these checks drive real SDL3 windows and its event
//! queue on every CI runner. A display-backed window is opened as well when one is available.

use dream_host_abi::window_surface::{WINDOW_PLATFORM_APPKIT, WindowSurface};
use dream_host_window::{dream_window_surface_v1, host, wire::kind};
use std::time::{Duration, Instant};

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

fn null_platform_pump() {
    host::initialize().expect("SDL3 dummy driver starts without a display");
    let id = host::open("null", 320, 200, 1 | 4).expect("null window");
    let query = host::query(id);
    assert_eq!(&query[..4], &[320.0, 200.0, 320.0, 200.0], "{query:?}");

    host::poll(id);
    host::set_size(id, 640, 480);
    let events = records(&host::poll(id));
    assert!(
        events.contains(&(kind::RESIZED, vec![640.0, 480.0])),
        "a size change reaches the next poll as a pixel-size Resized record: {events:?}"
    );
    assert!(
        records(&host::poll(id))
            .iter()
            .all(|(k, _)| *k != kind::RESIZED)
    );

    host::set_title(id, "renamed");
    host::set_cursor_mode(id, 2);
    host::set_cursor_mode(id, 0);
    host::set_cursor_icon(id, 3);
    host::set_cursor_icon(id, 99);
    host::set_fullscreen(id, true);
    host::poll(id);
    host::set_fullscreen(id, false);
    host::poll(id);

    let mut surface = WindowSurface::default();
    assert_eq!(
        unsafe { dream_window_surface_v1(id, &mut surface) },
        0,
        "the dummy driver has no native surface to hand to a GPU host"
    );
    assert_eq!(
        unsafe { dream_window_surface_v1(id, std::ptr::null_mut()) },
        0
    );

    park_and_wake(id);
    route_events(id);

    host::close(id);
    assert!(host::poll(id).is_empty());
    assert!(host::query(id).is_empty());
    assert_eq!(unsafe { dream_window_surface_v1(id, &mut surface) }, 0);
}

/// The scheduler's idle wait runs `host::park`, so it must return promptly on a wake from another
/// thread, honor its timeout, and leave the events it pumped for the next poll.
fn park_and_wake(id: i32) {
    host::poll(id);
    let waker = std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(50));
        host::wake();
    });
    let started = Instant::now();
    host::park(-1);
    let woke = started.elapsed();
    waker.join().unwrap();
    assert!(
        woke < Duration::from_secs(5),
        "a wake from another thread ends an indefinite park ({woke:?})"
    );

    host::poll(id);
    host::wake();
    let started = Instant::now();
    host::park(-1);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "a wake posted before parking is not lost"
    );

    host::poll(id);
    let started = Instant::now();
    host::park(20_000_000);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "park honors its timeout"
    );

    host::poll(id);
    host::set_size(id, 300, 200);
    host::park(10_000_000);
    let events = records(&host::poll(id));
    assert!(
        events.contains(&(kind::RESIZED, vec![300.0, 200.0])),
        "events pumped while parked reach the next poll: {events:?}"
    );
}

fn worker_threads_are_refused() {
    let refused = std::thread::spawn(|| {
        let open = host::open("worker", 10, 10, 0).map_err(|e| e.code);
        let mut surface = WindowSurface::default();
        let handed = unsafe { dream_window_surface_v1(1, &mut surface) };
        (open, host::poll(1), handed)
    })
    .join()
    .unwrap();
    assert_eq!(refused, (Err("ETHREAD"), Vec::new(), 0));
}

#[cfg(target_os = "macos")]
fn assert_on_main_thread() {
    assert_ne!(
        unsafe { libc::pthread_main_np() },
        0,
        "harness = false keeps main() on the main thread"
    );
}

#[cfg(not(target_os = "macos"))]
fn assert_on_main_thread() {}

fn main() {
    assert_on_main_thread();
    // SDL3 keeps its video driver for the whole process, so the
    // display-backed surface check replaces the Null run when a desktop session is available.
    if std::env::var_os("DREAM_WINDOW_DISPLAY_TEST").is_some() {
        display_surface();
    } else {
        // No SDL or other threads exist yet, so configuring the process driver is safe.
        unsafe { std::env::set_var("SDL_VIDEO_DRIVER", "dummy") };
        null_platform_pump();
    }
    worker_threads_are_refused();
    println!("dream-host-window main-thread checks passed");
}

fn display_surface() {
    let id = host::open("surface", 200, 100, 4).expect("display window");
    host::poll(id);
    let mut surface = WindowSurface::default();
    assert_eq!(unsafe { dream_window_surface_v1(id, &mut surface) }, 1);
    assert!(!surface.window.is_null() || surface.xlib_window != 0);
    assert_eq!(surface.vsync, 1);
    if cfg!(target_os = "macos") {
        assert_eq!(surface.platform, WINDOW_PLATFORM_APPKIT);
    }
    host::close(id);
}

fn route_events(first: i32) {
    use sdl3::{
        event::Event,
        sys::{
            stdinc::SDL_free,
            video::{SDL_GetWindowID, SDL_GetWindows},
        },
    };
    let second = host::open("second", 100, 100, 0).expect("second window");
    host::poll(first);
    host::poll(second);
    let mut count = 0;
    // SDL owns the windows; only the temporary pointer array belongs to this caller.
    let windows = unsafe { SDL_GetWindows(&mut count) };
    assert!(!windows.is_null());
    assert_eq!(count, 2);
    let native_ids: Vec<_> = unsafe { std::slice::from_raw_parts(windows, count as usize) }
        .iter()
        .map(|window| unsafe { SDL_GetWindowID(*window).0 })
        .collect();
    unsafe { SDL_free(windows.cast()) };
    let sdl = sdl3::init().expect("existing SDL");
    let events = sdl.event().expect("existing event subsystem");
    for id in &native_ids {
        events
            .push_event(Event::Window {
                timestamp: 0,
                window_id: *id,
                win_event: sdl3::event::WindowEvent::Moved(17, 29),
            })
            .unwrap();
    }
    assert!(records(&host::poll(first)).contains(&(kind::MOVED, vec![17.0, 29.0])));
    assert!(records(&host::poll(second)).contains(&(kind::MOVED, vec![17.0, 29.0])));
    assert!(
        !records(&host::poll(second))
            .iter()
            .any(|(kind, _)| *kind == kind::MOVED)
    );
    push_lifecycle(sdl3::sys::events::SDL_EVENT_WILL_ENTER_BACKGROUND);
    assert!(records(&host::poll(first)).contains(&(kind::OCCLUDED, vec![1.0])));
    assert!(records(&host::poll(second)).contains(&(kind::OCCLUDED, vec![1.0])));
    push_lifecycle(sdl3::sys::events::SDL_EVENT_DID_ENTER_FOREGROUND);
    assert!(records(&host::poll(first)).contains(&(kind::OCCLUDED, vec![0.0])));
    host::close(second);
}

fn push_lifecycle(kind: sdl3::sys::events::SDL_EventType) {
    let mut event = sdl3::sys::events::SDL_Event::default();
    event.r#type = kind.0;
    // Lifecycle events use only the common header, whose other fields are zero-initialized.
    assert!(unsafe { sdl3::sys::events::SDL_PushEvent(&mut event) });
}
