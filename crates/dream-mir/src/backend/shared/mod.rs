//! Backend pieces shared by WAT and native C (not wasm-specific).

pub(crate) mod panic_msgs;
mod symbols;
mod valuetype;

pub(crate) use symbols::{func_symbol, poll_symbol};
pub(crate) use valuetype::{is_simd_vector, ValueFrame, ValueLocalKind};
