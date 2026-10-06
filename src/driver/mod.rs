//! Compilation driver: wires the phases into an end-to-end build. [`compiler`] is the entry point
//! that runs lex -> parse -> analyze -> lower -> optimize -> emit and writes the module; the rest
//! are the cross-cutting pieces it orchestrates: [`source_loader`]/[`prelude`] (assembling user
//! sources with the bundled stdlib), [`interface_defaults`] (synthesizing inherited default-method
//! Layering: parse / prelude / class collection / interface defaults (inherit default-method
//! bodies before analysis), [`generate`] (source generators including `@json`),
//! [`abi`] (shared runtime layout constants), [`wasm_opt`] (opt-in Binaryen post-processing), and
//! [`error`] (the driver-level error type).

pub mod abi;
pub mod compiler;
pub mod compress;
pub mod diag_highlight;
pub mod error;
pub mod ffi_shim;
pub mod generate;
pub mod interface_defaults;
pub mod js_runtime;
#[cfg(feature = "native")]
pub mod metrics;
pub mod native_sets;
pub mod output;
pub mod prelude;
pub mod project_manifest;
pub mod quiet_panic;
pub mod rt_stamp;
pub mod source_loader;
pub mod target;
#[cfg(feature = "native")]
pub mod test;
pub mod toolchain;
pub mod ui;
pub mod wasi;
pub mod wasm_opt;
