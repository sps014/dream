//! Builtin `@json` derive: builds a declaration snapshot, runs the Dream `JsonGenerator`
//! harness (cached native build), and `emit_file`s the resulting `extend` source.

#[cfg(feature = "native")]
mod cache;
#[cfg(feature = "native")]
mod collection_discovery;
#[cfg(feature = "native")]
mod collection_expr;
#[cfg(feature = "native")]
mod collection_stmts;
#[cfg(feature = "native")]
mod collection_types;
#[cfg(feature = "native")]
mod diagnostics;
mod expand;
#[cfg(feature = "native")]
mod harness;
#[cfg(feature = "native")]
mod snapshot;

pub use expand::expand_from_acc;
