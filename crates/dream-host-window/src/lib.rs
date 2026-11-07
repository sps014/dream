//! Native windows and game input on SDL3; guest allocation and binding remain owned by core.
//!
//! Video and event pumping stay on the process main thread with Dream's scheduler. Only the
//! wake event can be posted from workers. See `docs/internals/17-window-host.md`.
mod exports;
mod gamepads;
pub mod host;
mod input;
mod keys;
mod metrics;
mod pump;
mod surface;
mod thread;
pub mod wire;

pub use surface::dream_window_surface_v1;

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_window_abi_v3() {}
