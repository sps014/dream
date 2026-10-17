//! Runs on the process main thread (`harness = false`), the only thread GLFW may use.
//!
//! GLFW's Null platform needs no display, so these checks drive real GLFW windows and its event
//! queue on every CI runner. A display-backed window is opened as well when one is available.

use dream_host_abi::window_surface::{WINDOW_PLATFORM_APPKIT, WindowSurface};
use dream_host_window::{dream_window_surface_v1, host, wire::kind};

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
    host::initialize(glfw::Platform::Null).expect("GLFW Null platform starts without a display");
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
        "the Null platform has no native surface to hand to a GPU host"
    );
    assert_eq!(
        unsafe { dream_window_surface_v1(id, std::ptr::null_mut()) },
        0
    );

    host::close(id);
    assert!(host::poll(id).is_empty());
    assert!(host::query(id).is_empty());
    assert_eq!(unsafe { dream_window_surface_v1(id, &mut surface) }, 0);
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
    // GLFW keeps the platform chosen at first initialization for the whole process, so the
    // display-backed surface check replaces the Null run when a desktop session is available.
    if std::env::var_os("DREAM_WINDOW_DISPLAY_TEST").is_some() {
        display_surface();
    } else {
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
