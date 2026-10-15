//! Portable data formats shared by native and WASM capability adapters.
mod codecs;
mod compression;
#[cfg(not(target_arch = "wasm32"))]
mod exports;
#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub extern "C" fn dream_host_data_abi_v2() {}
