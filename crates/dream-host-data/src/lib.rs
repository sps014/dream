//! Portable data formats shared by native and WASM capability adapters.
mod abi;
mod codecs;
#[cfg(feature = "compression")]
mod compression;
#[cfg(not(target_arch = "wasm32"))]
mod exports;
#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub extern "C" fn dream_host_data_abi_v3() {}
