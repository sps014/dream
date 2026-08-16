//! Native OS/library hosts (`extern "C"`) linked into LLVM binaries.
//!
//! Same wire formats as wasmtime `src/execution/host/`. Heap stays C (`i32` guest pointers).
//! Optional `http` / `net` / `gpu` / `webview` pull reqwest, tungstenite, wgpu, and wry.

mod crypto;
mod datetime;
mod file_handle;
mod process;
mod text;
mod worker;
mod task;
mod c_ffi;
mod heavy_stubs;

#[cfg(feature = "gpu")]
pub mod gpu;
#[cfg(feature = "http")]
mod http;
#[cfg(feature = "net")]
mod net;
#[cfg(feature = "webview")]
mod webview;
