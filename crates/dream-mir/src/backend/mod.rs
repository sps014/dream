//! Codegen backends: C99 for wasm32 (`c` with [`CTarget::Wasm32`]) and native hosts.

pub mod c;
pub(crate) mod shared;

pub use shared::print_wasm;

use crate::Mir;
use dream_abi::{js_abi, runtime_hosts};
use dream_types::TypeInterner;

/// True when this module needs WASM shared memory + atomics: `WebWorker` / pool host imports, or a
/// remaining `@shared class` layout (`Lock` / `Semaphore` / user shared types).
pub fn module_needs_threads(mir: &Mir, interner: &TypeInterner) -> bool {
    if mir.imports.iter().any(|imp| {
        imp.module == js_abi::HOST_MODULE && runtime_hosts::is_worker_host_field(&imp.field)
    }) {
        return true;
    }
    mir.layouts
        .structs
        .keys()
        .any(|ty| interner.is_shared_type(*ty))
}
