//! Builds through the LLVM backend: MIR → `.ll`, linked with the runtime bitcode, then `opt` +
//! `llc` from the pinned toolchain, and the system linker (native) or wasi-sdk `wasm-ld` (wasm32).

pub mod build;
pub mod runtime;
pub mod tools;
pub mod wasm;

pub use build::{compile_llvm, Toolchain};
pub use runtime::{llvm_runtime, LlvmRuntime};
pub use tools::{resolve_llvm, LlvmTools, LLVM_VERSION};
