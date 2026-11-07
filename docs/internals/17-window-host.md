# Window host: threading, event wire, and GPU surfaces

`system.window` uses SDL3 through `dream-host-window` on native platforms and
`runtime/src/hosts/window.js` in browsers. Dream owns its input state and drawing APIs; SDL owns
native windows, events, controller mappings, and cursor resources. Rendering remains in the GPU
and drawing capabilities.

## Thread ownership and scheduling

Dream's native entry point and cooperative scheduler run on the process main thread. Window
state lives in a thread-local host. macOS and iOS verify `pthread_main_np`, Linux verifies
`gettid() == getpid()`, and other platforms bind to the first host thread. A worker's
`Window.open` reports `ETHREAD`; other window calls from that worker return empty results or do
nothing. The host never marshals guest calls from workers to the window thread.

`windowPoll` pumps SDL events synchronously and routes them by SDL window id to each Dream window's
pending wire. Polling one window preserves events for all the others. No OS callback executes
Dream code. The host samples drawable size, display scale, safe area, and visibility each time it
pumps. Scale changes precede resize records so Dream derives the correct logical dimensions.

While windows exist, scheduler park hooks use SDL's timed event wait. The event consumed by a wait
is stored until the next pump. A reserved SDL user event wakes the wait from another thread. A
wake posted before parking remains queued, closing the race between checking the scheduler's
foreign queue and entering the platform wait. Closing the last window restores the scheduler's
normal wait. A parked scheduler services OS events, but drawing resumes only when its frame loop
runs again. Desktop modal OS operations can still delay guest execution.

## Platform-owned loop on Android and iOS

On phones the operating system owns the main thread, so Dream cannot block in
`dream_run_loop` there. `TargetSpec::hosts_event_loop` selects a different entry: the emitted
C `main` passes the guest entry and `dream_scheduler_pump` to the window host's
`dream_window_app_main` (`c_abi::PLATFORM_APP_MAIN`). Emission records that import, so every
mobile program links the window capability. The guest entry starts `main` and returns instead of
running the loop.

`dream_window_app_main` enters SDL's main callbacks through `SDL_RunApp`. On Android SDL's
`SDLActivity` loads the capability libraries and the program's `.so`, then calls its exported
`main` on SDL's thread; on iOS `SDL_RunApp` starts UIKit. The init callback runs the guest entry;
each iterate callback runs one pump and ends the app once nothing is pending. The quit callback
drops the window host. A second call returns 1 without re-entering SDL.

`dream_scheduler_pump` is one bounded pass: it drains foreign completions and callbacks, fires the
timers that were due when the pass began, and runs the ready queue until it empties. Work made ready
by completions that arrive during the pass, including GPU presents, resumes on the next pass, so a
frame loop yields to the platform once per frame. The pump never waits, returns 1 while work is
outstanding, and returns 1 immediately when called while a pump is already on the stack.
`dream_run_loop` remains the blocking desktop loop and the loop used by `Task` workers.

SDL's event callback may run inside an SDL call the guest made, so it never calls Dream. It stores
each event in the host's inbox, and the next `windowPoll` routes inbox events before newly polled
ones. Android's `JNI_OnLoad` belongs to the window capability: SDL's definition is renamed to
`dream_sdl_jni_onload` at build time (`.cargo/config.toml`), and the host exports a `JNI_OnLoad`
that forwards to it. Lifecycle ordering during Android surface replacement is not yet handled.

## Event wire and coordinates

`windowPoll` returns `[kind, length, payload...]` float records. `wire.rs` defines the kinds and
`system/window/input_wire.dream` decodes them. Text uses UTF-16 code units, exactly representable
as `f32`. Dream's `InputTracker` folds native, browser, and simulated events into the same input
levels and frame edges.

SDL window coordinates become Dream logical coordinates using display scale divided by pixel
density. Mouse motion, click positions, touch positions, window dimensions, and safe-area insets
therefore share one coordinate system. Drawable dimensions stay in physical pixels. Touch device
and finger ids are mapped to compact stable ids that fit the float wire exactly. Focus loss,
backgrounding, and hiding cancel outstanding touches. SDL's synthetic touch/mouse translation is
disabled to avoid duplicate input.

Physical SDL scancodes map to Dream's `KeyCode` order independently of keyboard layout. Text
input uses SDL text events. Desktop text input is enabled when a window opens; mobile text
editing and IME are deferred. Controller slots remain stable while connected, triggers range
from zero to one, sticks range from minus one to one, and rumble returns false when the device
or driver rejects it.

## GPU surface boundary

`dream_window_surface_v1` returns the layout in `dream_host_abi::window_surface`. The GPU host
links the window capability and queries this symbol; it does not call SDL. Its integer argument
is the internal Dream window handle. SDL native window properties are checked before exposing
any pointer.

The ABI identifies AppKit (`NSView*`), Win32 (`HWND` and `HINSTANCE`), Xlib (`Display*`, window id,
and screen), Wayland (`wl_surface*` and `wl_display*`), Android (`ANativeWindow*`), and UIKit
(`CAMetalLayer*`). These identifiers describe handles; each renderer must separately support them.
The pinned GPU library presents on AppKit, Win32, Xlib, Android (Vulkan), and UIKit (Metal). On
iOS the window host creates an `SDL_Metal_CreateView` for each window and exposes that view's
layer; patch `0005` lets the GPU library adopt a UIKit layer instead of creating one from an
AppKit `NSWindow`. Phones build the GPU library without Dawn. Linux
selects X11 by default, including XWayland sessions, because that library has no Wayland surface
implementation. An explicit Wayland driver can create a window but GPU surface creation fails.

The function returns zero for null output storage, dummy windows, unknown or closed handles,
backgrounded or hidden windows, and calls outside the window thread. Callers must re-query after
resize and lifecycle changes. Mobile native handles can be replaced by the OS and must never be
cached across a surface replacement.

## Build and verification

Cargo pins the Rust bindings and SDL source version. SDL is built from source with CMake and
linked statically into the window capability. Linux needs the development packages for its
selected SDL video and input backends, including `libxss-dev`: SDL refuses to configure X11
when the XScreenSaver headers are missing. Desktop GPU presentation uses X11/XWayland.

The `main_thread` harness runs without libtest worker threads. By default it selects SDL's dummy
video driver and checks window sizing, event preservation while parked, cross-thread wakeups,
closed handles, and worker refusal. `DREAM_WINDOW_DISPLAY_TEST=1` instead verifies a native
surface. Input unit tests cover logical click coordinates, relative mouse motion, compact touch
ids, cancellation, UTF-16 text, key codes, and modifiers. The `app_loop` harness drives
`dream_window_app_main` with the dummy driver and checks that events recorded by the event
callback reach `windowPoll` and that the loop ends when the guest finishes. The GPU backend harness adds actual
surface acquisition and presentation with `DREAM_GPU_SURFACE_TEST=1`.
