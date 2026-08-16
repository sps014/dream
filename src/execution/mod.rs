//! Host execution: [`native_runner`] for LLVM binaries (`dream run`), [`wasm_runner`] for wasm32
//! (playground / `--web`). [`host`] is the wasmtime import surface for wasm32 only.

pub mod debugger;
pub mod host;
pub mod native_runner;
pub mod wasm_runner;
