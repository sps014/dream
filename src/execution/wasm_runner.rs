use super::cwasm::{deserialize_module_file, module_from_bytes};
use super::host::{
    aot_wasm_config, attach_c_abi_from_json, attach_c_abi_from_wat_path, decode_string,
    define_env_memory, enable_ansi_support, link_c_ffi_imports, link_console_functions,
    link_crypto_functions, link_datetime_functions, link_ffi_helpers, link_file_functions,
    link_gpu_functions, link_http_functions, link_math_functions, link_net_functions,
    link_process_functions, link_text_functions, link_webview_functions, link_worker_functions,
    set_worker_runtime, with_guest_bytes,
};
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use wasmtime::*;

thread_local! {
    /// When `Some`, `print_*` / `println` append here instead of writing to process stdout.
    static PRINT_CAPTURE: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn execute_wasm(wat_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    run_wasm_path(wat_path, false)?;
    Ok(())
}

/// Like [`execute_wasm`], but captures all `print_*` / `println` output into the returned string
/// instead of writing to the process stdout. Used by compile-time Dream source generators.
pub fn execute_wasm_capturing(wat_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    PRINT_CAPTURE.with(|c| {
        *c.borrow_mut() = Some(String::new());
    });
    let result = run_wasm_path(wat_path, true);
    let captured = PRINT_CAPTURE.with(|c| c.borrow_mut().take().unwrap_or_default());
    result?;
    Ok(captured)
}

/// Run a compiled module from raw wasm (or wat) bytes — used by `dream-runner` packed binaries.
pub fn execute_wasm_bytes(wasm_or_wat: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    execute_wasm_bytes_with_abi(wasm_or_wat, None)
}

/// Like [`execute_wasm_bytes`], but installs `@c` / struct ABI from embedded `.abi.json` text so
/// packed hosts can marshal strings, `ref`, and callbacks without a sibling file on disk.
pub fn execute_wasm_bytes_with_abi(
    wasm_or_wat: &[u8],
    abi_json: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(text) = abi_json {
        if !text.is_empty() {
            attach_c_abi_from_json(text);
        }
    }
    run_wasm_bytes(wasm_or_wat, false, &[PathBuf::from(".")])?;
    Ok(())
}

fn run_wasm_path(wat_path: &str, capturing: bool) -> Result<(), Box<dyn std::error::Error>> {
    super::host::attach_abi_from_wat_path(wat_path);
    attach_c_abi_from_wat_path(wat_path);
    let search_roots = vec![std::path::Path::new(wat_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))];
    let config = aot_wasm_config();
    let engine = Engine::new(&config)?;
    let module = load_module_beside_wat(&engine, wat_path)?;
    run_loaded_module(engine, module, capturing, &search_roots)
}

fn run_wasm_bytes(
    wasm_bytes: &[u8],
    capturing: bool,
    search_roots: &[PathBuf],
) -> Result<(), Box<dyn std::error::Error>> {
    let config = aot_wasm_config();
    let engine = Engine::new(&config)?;
    let module = module_from_bytes(&engine, wasm_bytes)?;
    run_loaded_module(engine, module, capturing, search_roots)
}

/// Prefer a sibling `.cwasm` (Cranelift AOT), then `.wasm` (picks up wasm-opt), then parse `.wat`.
fn load_module_beside_wat(
    engine: &Engine,
    wat_path: &str,
) -> Result<Module, Box<dyn std::error::Error>> {
    let base = std::path::Path::new(wat_path);
    let cwasm_path = base.with_extension("cwasm");
    if cwasm_path.is_file() {
        match Engine::detect_precompiled_file(&cwasm_path)? {
            Some(wasmtime::Precompiled::Module) => {
                return Ok(deserialize_module_file(engine, &cwasm_path)?);
            }
            Some(wasmtime::Precompiled::Component) => {
                return Err("sibling .cwasm is a Wasmtime component, not a core module".into());
            }
            None => {}
        }
    }
    let wasm_path = base.with_extension("wasm");
    if wasm_path.is_file() {
        let bytes = fs::read(&wasm_path)?;
        return Ok(Module::new(engine, &bytes)?);
    }
    let wat_content = fs::read_to_string(wat_path)?;
    let wasm_bytes = wat::parse_str(&wat_content)?;
    Ok(Module::new(engine, &wasm_bytes)?)
}

fn run_loaded_module(
    engine: Engine,
    module: Module,
    capturing: bool,
    search_roots: &[PathBuf],
) -> Result<(), Box<dyn std::error::Error>> {
    enable_ansi_support();

    // A recursive ARC release (e.g. dropping a long `Option<T>`-boxed linked list) chains one wasm
    // frame per node through both the struct's and its `Option` wrapper's release function, so the
    // default 512 KiB wasm stack undersizes for large-but-ordinary data structures; size up via
    // `DREAM_STACK_SIZE` / `[package.metadata.dream] stack-size` (see `host::stack_size`).
    // `aot_wasm_config` enables the WASM threads proposal so modules that *do* import shared
    // memory can still instantiate. Linear memory is shared only when the guest declares
    // `shared` (WebWorker / atomics); otherwise `define_env_memory` installs a private `Memory`.

    // Threaded runs share one `SharedMemory` across the owner instance and every `WebWorker`
    // spawned afterward (`set_worker_runtime`). Non-threaded modules skip that path.
    let mut store = Store::new(&engine, ());
    store.set_epoch_deadline(u64::MAX);
    let mut linker = Linker::new(&engine);
    link_host_functions(&mut linker)?;
    if let Some(shared_mem) = define_env_memory(&engine, &mut store, &mut linker, &module)? {
        set_worker_runtime(engine.clone(), shared_mem, module.clone());
    }

    link_c_ffi_imports(&mut linker, &module, search_roots)?;

    // JS-interop externs (e.g. the `Dream` host module behind the dynamic `js` type/regex/fetch, or any
    // user `@js(...)` import) have no native implementation. Stub every still-unresolved import
    // as a trap so modules that merely *declare* them still instantiate and run under wasmtime;
    // calling one without a JS host traps, matching `runtime/dream.js`'s thrower stubs.
    linker.define_unknown_imports_as_traps(&module)?;

    let instance = linker.instantiate(&mut store, &module)?;

    if let Ok(main_func) = instance.get_typed_func::<(), ()>(&mut store, dream_mir::abi::ENTRY_FN) {
        main_func.call(&mut store, ())?;
    } else if !capturing {
        println!("No main function found in module");
    }

    Ok(())
}

fn append_capture(text: &str) -> bool {
    PRINT_CAPTURE.with(|c| {
        if let Some(buf) = c.borrow_mut().as_mut() {
            buf.push_str(text);
            true
        } else {
            false
        }
    })
}

/// Wires every fixed host binding a compiled Dream module may import — the `print_*` builtins and the
/// [`super::host`] function modules (math/file/http/regex/console/datetime/worker) — into `linker`.
/// Shared by the normal runner and the interactive debugger so both expose an identical host ABI.
pub fn link_host_functions(linker: &mut Linker<()>) -> Result<()> {
    link_print_functions(linker)?;
    link_runtime_host_functions(linker)?;
    link_noop_debug_hooks(linker)?;
    Ok(())
}

/// No-op `dream_debug` hooks for the normal (non-debugger) runner. A module compiled with `-g`
/// imports `dream_debug.enter/line/exit`; without an attached debugger these must resolve to
/// harmless no-ops rather than trapping via `define_unknown_imports_as_traps`. The interactive
/// debugger deliberately does *not* call this — it links its own hooks that pause execution.
pub fn link_noop_debug_hooks(linker: &mut Linker<()>) -> Result<()> {
    linker.func_wrap("dream_debug", "enter", |_id: i32| {})?;
    linker.func_wrap("dream_debug", "exit", |_id: i32| {})?;
    linker.func_wrap("dream_debug", "line", |_file: i32, _line: i32| {})?;
    Ok(())
}

/// Wires the `print_*`/`println` builtins to real process stdout (or a thread-local capture buffer
/// when [`execute_wasm_capturing`] is active). The debugger provides its own variants that forward
/// to DAP `output` events instead.
pub fn link_print_functions(linker: &mut Linker<()>) -> Result<()> {
    linker.func_wrap("env", "print_int", |v: i32| {
        let s = format!("{}", v);
        if !append_capture(&s) {
            print!("{}", s);
        }
    })?;

    linker.func_wrap("env", "print_float", |v: f32| {
        let s = format!("{}", v);
        if !append_capture(&s) {
            print!("{}", s);
        }
    })?;

    linker.func_wrap("env", "print_double", |v: f64| {
        let s = format!("{}", v);
        if !append_capture(&s) {
            print!("{}", s);
        }
    })?;

    linker.func_wrap("env", "print_char", |v: i32| {
        if let Some(c) = char::from_u32(v as u32) {
            let s = format!("{}", c);
            if !append_capture(&s) {
                print!("{}", s);
            }
        }
    })?;

    linker.func_wrap(
        "env",
        "print_string",
        |mut caller: Caller<'_, ()>, ptr: i32| -> Result<()> {
            let s = with_guest_bytes(&mut caller, |data| decode_string(data, ptr))?;
            if !append_capture(&s) {
                print!("{}", s);
            }
            Ok(())
        },
    )?;

    linker.func_wrap(
        "env",
        "println",
        |mut caller: Caller<'_, ()>, ptr: i32| -> Result<()> {
            let s = with_guest_bytes(&mut caller, |data| decode_string(data, ptr))?;
            if !append_capture(&(s.clone() + "\n")) {
                println!("{}", s);
            }
            Ok(())
        },
    )?;
    Ok(())
}

/// Wires every host binding *except* the `print_*` builtins: the [`super::host`] function modules
/// (math/file/http/console/datetime/worker) plus the `strlen`/`debug_get_free_list_head`
/// stubs. The debugger reuses this and supplies its own print bindings.
pub fn link_runtime_host_functions(linker: &mut Linker<()>) -> Result<()> {
    link_math_functions(linker)?;
    link_file_functions(linker)?;
    link_http_functions(linker)?;
    link_crypto_functions(linker)?;
    link_console_functions(linker)?;
    link_datetime_functions(linker)?;
    link_process_functions(linker)?;
    link_net_functions(linker)?;
    link_text_functions(linker)?;
    link_worker_functions(linker)?;
    link_gpu_functions(linker)?;
    link_webview_functions(linker)?;
    #[cfg(feature = "c-ffi")]
    link_ffi_helpers(linker)?;
    linker.func_wrap("env", "strlen", |_: i32| -> i32 { 0 })?;
    linker.func_wrap("env", "debug_get_free_list_head", || -> i32 { 0 })?;
    Ok(())
}
