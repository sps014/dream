//! Codegen policy the LLVM writers consume: layouts, symbol and string tables, ABI value
//! classes, ARC glue naming, entry-exit and protocol reachability. The writers only decide how to
//! print what this layer decides.

pub(crate) mod abi_types;
pub(crate) mod cx;
pub(crate) mod entry;
pub(crate) mod glue;
pub(crate) mod iface_guard;
pub(crate) mod js_marshal;
pub(crate) mod panic_msgs;
pub(crate) mod place_policy;
mod print;
pub(crate) mod protocol_names;
pub(crate) mod reach;
mod symbols;
pub(crate) mod tables;
mod target;
mod valuetype;

pub use print::print_wasm;
pub(crate) use symbols::func_symbol;
pub use target::Target;
pub(crate) use valuetype::{ValueFrame, ValueLocalKind};
