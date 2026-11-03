//! Whole-module tree-shaking: reachability-based function pruning plus the symbol-table shaking that
//! rides on it (dead globals, unused layouts, and unreferenced `extern` imports).
//!
//! The analyzer skips library bodies it can prove unreachable, but it still lowers every user
//! function and every library body some reachable HIR names. This pass computes what is reachable
//! from the entry points and drops the rest, so the backend never has to resolve dead code (which
//! may reference runtime pieces the MIR backend has not wired yet) and the emitted module stays
//! small. [`dead_code`] drops unreachable functions, dead globals, unused layouts, and
//! unreferenced `extern` imports; an `async` body's edges come from its preserved HIR through
//! [`dream_hir::hir_body_edges`].

use dream_types::{DefId, TypeId};

mod dead_code;

pub(crate) use dead_code::module_uses_js_bridges;
pub use dead_code::prune_module;
pub(crate) use dream_hir::{HirEdges, hir_body_edges};

/// Identity of a function/instance for the call graph: its def plus the concrete type-args of the
/// monomorphized instance (empty for non-generic functions), matching `MirFunction::{def, instance}`
/// and `Callee::{def, args}`.
type FnKey = (DefId, Vec<TypeId>);
