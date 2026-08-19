//! Backend pieces shared by WAT and native C (not wasm-specific).

pub(crate) mod panic_msgs;
pub(crate) mod rc_store;
mod symbols;
mod valuetype;

pub(crate) use rc_store::unique_container_move_local;
pub(crate) use symbols::{func_symbol, poll_symbol};
pub(crate) use valuetype::{is_simd_vector, ValueFrame, ValueLocalKind};
