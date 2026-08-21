//! Guest C runtime sources and the [`modules`] catalog.

pub mod modules;

pub use modules::{
    native_pcre2_include_dir, native_runtime_c_files, native_runtime_include_dir,
    native_runtime_units, runtime_need_from_c_source, runtime_need_from_keys,
    runtime_need_from_mir, runtime_abi_include_dir, wasm32_heap_c, wasm32_libc_c,
    wasm32_linked_units, wasm32_runtime_c_files, wasm32_runtime_include_dir,
    NativeCompileUnit, RuntimeModule, RuntimeNeed, RUNTIME_MODULES, Wasm32LinkedUnit,
};
