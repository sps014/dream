//! Builds through the LLVM backend: MIR → `.ll`, linked with the runtime bitcode, then `opt` +
//! `llc` from the pinned toolchain, and the system linker (native) or LLVM's `wasm-ld` (wasm32).

pub mod build;
pub mod bundle;
mod c_shim;
pub mod cross;
pub mod doctor;
mod icon;
pub mod pack;
mod remarks;
pub mod runtime;
mod runtime_cache;
mod runtime_objects;
mod runtime_snapshot;
mod signatures;
pub mod tools;
pub mod wasm;
pub mod wasm_opt;
mod wasm_sources;

pub use build::{NativeBuildOptions, Toolchain, compile_llvm};
pub use pack::pack_runtime;
pub use runtime::{LlvmRuntime, llvm_runtime};
pub use tools::{LLVM_VERSION, LlvmTools, resolve_llvm};
