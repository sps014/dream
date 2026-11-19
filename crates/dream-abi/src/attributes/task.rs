//! Task attributes: which callees run their function arguments on another worker thread.

use super::*;

pub const WORKER_BODY: &str = "worker_body";

/// True when the callee runs its function-typed arguments on another worker, so their captures
/// must be `shared` or moved (`@worker_body`).
pub fn is_worker_body(attributes: &[AttributeNode]) -> bool {
    has_named_attr(attributes, WORKER_BODY)
}
