//! Native OS/library hosts (`extern "C"`) linked into LLVM binaries.
//!
//! Same wire formats as wasmtime `src/execution/host/`. Heap stays C (`i32` guest pointers).

mod crypto;
mod datetime;
mod file_handle;
pub mod gpu;
mod http;
mod net;
mod process;
mod text;
mod worker;
mod webview;
mod c_ffi;
