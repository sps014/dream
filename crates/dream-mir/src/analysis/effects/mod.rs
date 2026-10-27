//! Heap effects shared by ownership and allocation analyses.

mod cleanup;
mod closure;
mod elements;
#[cfg(test)]
#[path = "closure_tests.rs"]
mod closure_tests;
mod intrinsics;
mod local;
mod model;
mod table;

pub(crate) use cleanup::strong_children;
pub(crate) use elements::element_writes;
pub(crate) use local::{Effect, call_arguments, stmt_effects};
pub(crate) use model::{Known, ModRef};
pub(crate) use table::ModRefTable;
