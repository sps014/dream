//! Shared WASM linear memory wiring (WASM threads proposal). Threaded modules import
//! `env.memory` as `shared` so the owner instance and every spawned `WebWorker` of the same
//! module share one `wasmtime::SharedMemory`. Modules without threads import a private
//! `wasmtime::Memory` instead.
//!
//! `src/mir/emit/module.rs` emits linear memory as an *import* (`(import "env" "memory" (memory min
//! max shared))`) rather than a module-defined memory specifically so this crossing is possible: a
//! `wasmtime::SharedMemory` can only be handed to multiple `Instance`s if the module imports its
//! memory instead of instantiating its own.

use super::stack_size::{dream_async_stack_size, dream_stack_size};
use std::path::Path;
use wasmtime::*;

/// Engine config shared by JIT load and Cranelift AOT (`.cwasm`). Fingerprint must match between
/// [`crate::execution::cwasm::precompile_wasm`] and `Module::deserialize` — do not add flags here
/// that the AOT path omits.
///
/// Stack bytes come from [`dream_stack_size`] (`DREAM_STACK_SIZE` env, else Cargo.toml
/// `[package.metadata.dream] stack-size`, else 16 MiB).
pub fn aot_wasm_config() -> Config {
    let mut config = Config::new();
    config.max_wasm_stack(dream_stack_size());
    config.async_stack_size(dream_async_stack_size());
    config.wasm_threads(true);
    config.wasm_simd(true);
    config.wasm_relaxed_simd(true);
    config.wasm_tail_call(true);
    config.wasm_bulk_memory(true);
    config.wasm_multi_memory(true);
    config.wasm_extended_const(true);
    config.shared_memory(true);
    // Off: wasm_gc, wasm_exceptions, wasm_function_references, wasm_shared_everything_threads,
    // wasm_memory64 — must stay aligned with the wasm-opt allow-list in `src/driver/wasm_opt.rs`.
    // Hard `WebWorker.terminate()` aborts an in-flight body via `Engine::increment_epoch` (see
    // `host::worker`). Owner stores must call `set_epoch_deadline(u64::MAX)` so they are not
    // interrupted when a worker is killed.
    config.epoch_interruption(true);
    // Wasmtime's default parallel compilation submits codegen to rayon's *global* thread pool —
    // the same pool a caller may already be using for outer parallelism (e.g. `tests/e2e_tests.rs`).
    // If every pool thread is blocked in `rayon::join`, a nested `Module::new` deadlocks. Serial
    // compilation avoids that; workers now clone a compiled `Module` so this is cheap either way.
    config.parallel_compilation(false);
    config
}

/// [`aot_wasm_config`] plus optional Cranelift IR dump (`DREAM_EMIT_CLIF=/path/to/dir`).
///
/// Do not use this Engine to deserialize `.cwasm`: `emit_clif` changes the config fingerprint.
pub fn threaded_wasm_config() -> Config {
    let mut config = aot_wasm_config();
    if let Ok(dir) = std::env::var("DREAM_EMIT_CLIF") {
        if !dir.is_empty() {
            config.emit_clif(Path::new(&dir));
        }
    }
    config
}

/// Allocates a `SharedMemory` matching the `"env"."memory"` import type declared by `module` — the
/// single backing store every instance of this module (the owner instance and every worker thread
/// spawned afterward) imports, so linear memory is genuinely shared rather than copied.
pub fn env_memory_type(module: &Module) -> Result<MemoryType> {
    for import in module.imports() {
        if import.module() == "env" && import.name() == "memory" {
            if let ExternType::Memory(mt) = import.ty() {
                return Ok(mt);
            }
        }
    }
    Err(Error::msg("module does not import `env.memory`"))
}

/// Allocates a `SharedMemory` matching the `"env"."memory"` import when that import is shared.
pub fn shared_memory_for(engine: &Engine, module: &Module) -> Result<SharedMemory> {
    let mt = env_memory_type(module)?;
    if !mt.is_shared() {
        return Err(Error::msg(
            "module does not import `env.memory` as a shared memory",
        ));
    }
    SharedMemory::new(engine, mt)
}

/// Defines `"env"."memory"` as shared or private to match the guest import. Returns the
/// `SharedMemory` when the module uses threads; `None` for a private linear memory.
pub fn define_env_memory<T>(
    engine: &Engine,
    store: &mut Store<T>,
    linker: &mut Linker<T>,
    module: &Module,
) -> Result<Option<SharedMemory>> {
    let mt = env_memory_type(module)?;
    if mt.is_shared() {
        let sm = SharedMemory::new(engine, mt)?;
        linker.define(&mut *store, "env", "memory", sm.clone())?;
        Ok(Some(sm))
    } else {
        let mem = Memory::new(&mut *store, mt)?;
        linker.define(&mut *store, "env", "memory", mem)?;
        Ok(None)
    }
}
