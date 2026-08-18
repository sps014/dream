//! Guest C/WAT runtime sources and the [`modules`] catalog.

pub mod modules;

pub use modules::{
    allocate_linked_layouts, manifest_json, native_pcre2_include_dir, native_runtime_c_files,
    native_runtime_include_dir, native_runtime_units, runtime_need_from_c_source,
    runtime_need_from_keys, runtime_need_from_mir, LinkedLayout, NativeCompileUnit, RuntimeModule,
    RuntimeNeed, LINKED_REGION_ORIGIN, RUNTIME_MODULES,
};
