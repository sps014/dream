//! Heap effects shared by ownership and allocation analyses.

mod cleanup;
mod closure;
#[cfg(test)]
#[path = "closure_tests.rs"]
mod closure_tests;
mod intrinsics;
mod local;
mod model;
mod table;

pub(crate) use cleanup::strong_children;
pub(crate) use local::{Effect, stmt_effects};
pub(crate) use model::{Known, ModRef};
pub(crate) use table::ModRefTable;
