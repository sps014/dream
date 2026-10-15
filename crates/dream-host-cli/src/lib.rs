//! Shared clap parser for native and WebAssembly capability adapters.
mod command;
#[cfg(not(target_arch = "wasm32"))]
mod exports;
mod parse;
mod schema;
#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(not(target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub extern "C" fn dream_host_cli_abi_v2() {}

#[cfg(test)]
mod tests;
