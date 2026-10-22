# Window host: threading, event wire, and the GPU surface ABI

`system.window` ships native windows through the `dream-host-window` capability library, built on
GLFW 3.4, and browser windows through `runtime/src/hosts/window.js`. This note records the design
decisions that are not obvious from the code.

## The main-thread pump

macOS lets only the process main thread touch AppKit windows. GLFW also requires the main thread
for event processing on every platform.

On native targets, the generated C `main` calls `dream_guest_entry`. That function runs Dream
`main` and then `dream_run_loop`, the async scheduler, on the process main thread
(`crates/dream-mir/src/backend/llvm/glue/entry.rs`). The runtime starts other threads only for
`Task` workers and blocking I/O. Every guest call outside a `Task` therefore already arrives on
the main thread, so the host needs no thread of its own:

- `windowPoll` calls `glfwPollEvents` synchronously. GLFW's callbacks fill each window's receiver
  queue (the `glfw` crate's per-window channel), and the same call drains that queue into the
  returned array. No event crosses a thread.
- All host state lives in a thread-local on the window thread. The host checks
  `pthread_main_np` on macOS and `gettid() == getpid()` on Linux. On other platforms it binds to
  the first thread that used it. A call from any other thread changes nothing. `windowOpen`
  reports `ETHREAD`, and the other calls do nothing or return empty results.
- The host does not forward work from a `Task` to the main thread. Doing so would need a
  main-thread run loop that the guest scheduler does not own. On macOS a blocked main thread
  would also deadlock AppKit.

### Parking inside the window system

When nothing is ready, `dream_run_loop` parks the thread until a timer is due or a foreign
completion arrives. A plain condvar wait would leave the OS event queue unserviced, and the
platform would mark a window "not responding" during a slow `await`. The scheduler therefore
accepts park hooks through the guest context's `set_park` callback, which hosts reach as
`dream_host_set_park` in the core library (`dream_host_abi::set_scheduler_park`):

- While at least one window is open, the window host installs `park` =
  `glfwWaitEventsTimeout` (`glfwWaitEvents` without a timer) and `wake` = `glfwPostEmptyEvent`.
  Closing the last window restores the condvar wait.
- The scheduler re-checks its foreign queue under its mutex, unlocks, and calls `park`. Foreign
  completions and callback wakes call `wake` under the same mutex after signalling the condvar.
  A wake that lands between the re-check and the GLFW wait leaves an empty event queued, so the
  wait returns at once and no wakeup is lost.
- Events GLFW receives while parked go into the per-window receiver queues, and the next
  `windowPoll` returns them.
- Hooks are per scheduler (one per thread), and only the main thread installs them.

Consequences:

- An `await` that parks the scheduler, such as a slow network read, keeps the window responsive,
  but the frame loop does not draw until it resumes. Run the frame loop as its own task and start
  background I/O with `Promise.start`, checking its result each frame.
- While the user live-resizes a window, macOS runs a modal loop inside `glfwPollEvents`, so the
  guest does not run until the resize ends. This is a known GLFW limitation.
- `InitHint::CocoaChdirResources(false)` keeps the working directory unchanged when a program is
  packaged as an `.app` bundle.

The harness test `crates/dream-host-window/tests/main_thread.rs` uses `harness = false`, so its
checks run on the process main thread. It covers five things:

- It asserts `pthread_main_np` on macOS.
- It drives real GLFW windows on the display-free Null platform. Setting a size produces a
  `Resized` record on the next poll.
- `host::park` returns promptly on a wake from another thread, keeps a wake posted before it
  started, honors its timeout, and leaves the events it pumped for the next poll.
- It checks that a worker thread is refused.
- With `DREAM_WINDOW_DISPLAY_TEST=1`, it opens a real window and checks the AppKit surface handle.

## Event wire

`windowPoll` returns `float[]` records of the form `[kind, length, payload...]`.
`crates/dream-host-window/src/wire.rs` defines the kinds, and `system/window/input_wire.dream`
decodes them. Text is stored as UTF-16 code units, which `f32` represents exactly, so one
allocation carries every event of a frame.

The hosts report only events and gamepad samples. `InputTracker`, written in Dream and shared by
every target, folds them into levels and per-frame edges. Native, browser and `Window.simulate`
input therefore follow the same `pressed`/`released` rules. Two other conversions also happen in
the hosts:

- Native hosts convert GLFW screen coordinates to logical pixels. Win32 and X11 report pixels;
  macOS reports points.
- Native hosts rescale GLFW trigger axes from -1..1 to 0..1. Browser triggers come from the
  standard mapping's analog button values.

## GPU surface ABI

A GPU capability creates surfaces from handles that the window host exports. It never calls GLFW.
`dream_host_abi::window_surface` holds the shared `#[repr(C)]` layout:

```c
typedef struct DreamWindowSurface {
    uint32_t platform;      /* 1 AppKit, 2 Win32, 3 Xlib, 4 Wayland */
    uint32_t vsync;         /* 1 when WindowDesc.vsync(true) */
    void *window;           /* NSView* | HWND | wl_surface* | NULL for Xlib */
    void *display;          /* NULL | HINSTANCE | wl_display* | Display* */
    unsigned long xlib_window;
    int32_t xlib_screen;
    uint32_t pixel_width;
    uint32_t pixel_height;
    float scale;            /* physical pixels per logical pixel */
} DreamWindowSurface;

/* Exported by libdream_host_window. Returns 1 and fills *out for a live native window. */
int32_t dream_window_surface_v1(int32_t window, DreamWindowSurface *out);
```

The `window` argument is the guest handle. Stdlib code reads it through the internal
`Window.host_handle` getter, and `Window.vsync_hint` mirrors the vsync flag. The function returns
`0` in these cases:

- the window is headless, closed or unknown
- GLFW runs on the Null platform
- `out` is null
- the call is made off the window thread

The handles stay valid until the guest closes the window. Re-query after each `Resized` event to
get the new pixel size.

The symbol is deliberately absent from `HostCapability::Window.fields()`, because guests never
import it. A GPU host links `dream_host_window` next to `dream_host_core`.

## Building

GLFW is compiled from the `glfw-sys` crate's vendored sources (`src-build`), so the build needs
CMake and never downloads binaries. Linux builds enable only the X11 backend and need the X11
development headers (`libx11-dev libxcursor-dev libxi-dev libxinerama-dev libxrandr-dev`).
Wayland sessions run the windows through XWayland.
