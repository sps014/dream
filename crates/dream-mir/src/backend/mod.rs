//! Codegen backends: WAT/Wasmtime (`wasm`) and native C (`c`).

pub mod c;
pub(crate) mod shared;
pub mod wasm;
