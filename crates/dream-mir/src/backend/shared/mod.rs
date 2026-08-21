//! Backend pieces shared by the C targets (not target-specific).

pub(crate) mod panic_msgs;
mod print;
pub(crate) mod rc_store;
mod symbols;
mod valuetype;

pub use print::print_wasm;
pub(crate) use rc_store::unique_container_move_local;
pub(crate) use symbols::func_symbol;
pub(crate) use valuetype::{ValueFrame, ValueLocalKind};
