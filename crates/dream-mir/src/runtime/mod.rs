//! Guest C runtime sources and the [`modules`] catalog.

pub mod modules;

pub use modules::{
    NativeCompileUnit, RUNTIME_MODULES, RuntimeModule, RuntimeNeed, SOURCE_RUNTIME_C_DIR,
    WASM32_LIBC_UNITS, Wasm32LinkedUnit, core_runtime_include_dir, native_runtime_units,
    runtime_abi_include_dir, runtime_need_from_keys, runtime_need_from_mir,
    runtime_need_from_module_text, wasm32_linked_units, wasm32_runtime_c_files,
    wasm32_runtime_include_dir, wasm32_unit_defines_libc,
};
