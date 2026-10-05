//! Function inlining.
//!
//! Replaces a direct call with a copy of the callee's body, wired into the caller's CFG: the caller
//! block is split at the call, the callee's locals/blocks are renumbered into the caller, parameters
//! are bound to the argument operands, and every callee `Return` becomes a jump to a continuation
//! block (assigning the returned value into the call's destination first).
//!
//! Inlining runs as a [`crate::passes::ModulePass`] *after* module-wide [`crate::passes::rc::RcInsertion`]
//! (see `optimize_module_opts`): each callee already carries its scope-exit `Release`s. Returned
//! reference tokens transfer explicitly to the call destination, including container slots.
//! Value-struct teardown is emitter-side for
//! standalone functions; the inliner inserts [`crate::Statement::ValueDrop`] at each remapped
//! return→continuation edge so owning value locals still die at the call site (not the caller's
//! frame exit). Call-result dests are forced Owning (`__vret`) so the return `Assign` deep-copies
//! rather than Borrow-rebinding a synthetic temp.

pub(crate) mod graph;
mod remap;
#[cfg(test)]
mod return_tests;

mod eligibility;
mod pipeline;
mod splice;
#[cfg(test)]
mod tests;

/// A function's call-graph identity (matches `MirFunction::{def, instance}` and `Callee::{def,args}`).
type FnKey = (dream_types::DefId, Vec<dream_types::TypeId>);

pub use pipeline::Inliner;
