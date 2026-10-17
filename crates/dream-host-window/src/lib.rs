//! Native windows and game input on GLFW; guest allocation and binding remain owned by core.
//!
//! Every GLFW call happens on the process main thread, where Dream's `main` and its async
//! scheduler run. See `docs/internals/17-window-host.md`.
mod exports;
pub mod host;
mod keys;
mod surface;
mod thread;
pub mod wire;

pub use surface::dream_window_surface_v1;

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_window_abi_v2() {}
