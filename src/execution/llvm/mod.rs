//! Builds through the LLVM backend: MIR → `.ll`, linked with the runtime bitcode, then `opt` +
//! `llc` from the pinned toolchain, and the system linker (native) or LLVM's `wasm-ld` (wasm32).

pub mod build;
pub mod bundle;
mod c_shim;
pub mod cross;
pub mod doctor;
mod icon;
pub mod pack;
pub mod runtime;
pub mod tools;
pub mod wasm;
mod wasm_sources;
pub mod wasm_opt;

pub use build::{compile_llvm, NativeBuildOptions, Toolchain};
pub use pack::pack_runtime;
pub use runtime::{llvm_runtime, LlvmRuntime};
pub use tools::{resolve_llvm, LlvmTools, LLVM_VERSION};
