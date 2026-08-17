//! Cranelift AOT via Wasmtime `.cwasm` artifacts.
//!
//! `Engine::precompile_module` runs the same Cranelift pipeline `Module::new` uses, then serializes
//! machine code. `Module::deserialize` mmap's that blob at startup so packed apps skip codegen.

use super::host::aot_wasm_config;
use std::path::Path;
use wasmtime::{Engine, Module, Precompiled};

/// Compile `wasm` (binary or WAT) to a Wasmtime precompiled module for `target` (host if `None`).
pub fn precompile_wasm(
    wasm: &[u8],
    target: Option<&str>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let engine = engine_for_aot(target)?;
    Ok(engine.precompile_module(wasm)?)
}

/// Read `wasm_path`, AOT-compile, write `cwasm_path`.
pub fn write_cwasm_file(
    wasm_path: &Path,
    cwasm_path: &Path,
    target: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let wasm = std::fs::read(wasm_path)?;
    let bytes = precompile_wasm(&wasm, target)?;
    std::fs::write(cwasm_path, &bytes)?;
    Ok(())
}

/// Host-ISA `.cwasm` next to a finalized `.wasm` (after wasm-opt + ABI embed).
pub fn write_host_cwasm(wasm_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    write_cwasm_file(wasm_path, &wasm_path.with_extension("cwasm"), None)
}

pub fn engine_for_aot(target: Option<&str>) -> Result<Engine, Box<dyn std::error::Error>> {
    let mut config = aot_wasm_config();
    if let Some(triple) = target {
        config
            .target(triple)
            .map_err(|e| format!("wasmtime Config::target({triple}): {e:#}"))?;
    }
    Ok(Engine::new(&config)?)
}

/// Load a module from precompiled `.cwasm` bytes or raw wasm/WAT.
///
/// Precompiled blobs must have been produced by [`precompile_wasm`] / [`write_cwasm_file`] with a
/// matching [`aot_wasm_config`] (same Wasmtime version, features, and stack sizes).
pub fn module_from_bytes(
    engine: &Engine,
    bytes: &[u8],
) -> Result<Module, Box<dyn std::error::Error>> {
    match Engine::detect_precompiled(bytes) {
        Some(Precompiled::Module) => deserialize_module(engine, bytes).map_err(|e| e.into()),
        Some(Precompiled::Component) => {
            Err("precompiled Wasmtime component is not a Dream core module".into())
        }
        None => Ok(Module::new(engine, bytes)?),
    }
}

pub fn deserialize_module(engine: &Engine, bytes: &[u8]) -> wasmtime::Result<Module> {
    // SAFETY: `bytes` are a Wasmtime precompiled module we produced (`precompile_module`) or
    // embedded at `dreamer pack` time. `detect_precompiled` already matched a core-module header.
    unsafe { Module::deserialize(engine, bytes) }
}

pub fn deserialize_module_file(engine: &Engine, path: &Path) -> wasmtime::Result<Module> {
    // SAFETY: `path` is a sibling `.cwasm` written by this compiler or a pack-time embed.
    unsafe { Module::deserialize_file(engine, path) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmtime::Engine;

    const ADD_WAT: &str = r#"(module
        (func (export "add") (param i32 i32) (result i32)
            (i32.add (local.get 0) (local.get 1))))"#;

    #[test]
    fn precompile_roundtrip_deserializes() {
        let wasm = wat::parse_str(ADD_WAT).expect("wat");
        let cwasm = precompile_wasm(&wasm, None).expect("precompile");
        assert!(matches!(
            Engine::detect_precompiled(&cwasm),
            Some(Precompiled::Module)
        ));
        let engine = Engine::new(&aot_wasm_config()).expect("engine");
        let module = module_from_bytes(&engine, &cwasm).expect("deserialize");
        assert!(module.get_export("add").is_some());
    }
}
