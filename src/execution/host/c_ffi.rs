//! Native C FFI host: resolves `@c("lib", "symbol")` imports via libloading + libffi.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::path::{Path, PathBuf};

use libffi::middle::{arg, Cif, Closure, CodePtr, Type};
use libffi::low::{ffi_cif, Callback as FfiCallback};
use libloading::Library;
use serde::Deserialize;
use std::cell::Cell;
use std::ffi::c_void;
use wasmtime::*;

use super::memory::{required_memory, shared_bytes, shared_bytes_mut};

/// Set for the duration of a native C call so Dream `fun` callbacks can re-enter the current
/// wasmtime [`Caller`]. Only valid while blocked inside `ffi_call`.
thread_local! {
    static ACTIVE_CALLER: Cell<*mut Caller<'static, ()>> = const { Cell::new(std::ptr::null_mut()) };
}

/// Userdata for a libffi closure that forwards into a Dream funcref-table entry.
struct DreamCbData {
    funcidx: u32,
    /// WASM value kinds for each C arg: `"i32"`, `"i64"`, `"f32"`, `"f64"`, `"ptr"` (as i64).
    arg_kinds: Vec<&'static str>,
    /// `"i32"`, `"i64"`, `"void"`, …
    ret_kind: &'static str,
}

#[derive(Debug, Clone, Deserialize)]
struct AbiFile {
    #[serde(default)]
    externs: Vec<ExternMeta>,
    #[serde(default)]
    structs: HashMap<String, StructMeta>,
}

#[derive(Debug, Clone, Deserialize)]
struct ExternMeta {
    module: String,
    field: String,
    #[serde(default)]
    params: Vec<String>,
    #[serde(default)]
    result: String,
    #[serde(default)]
    kind: String,
    #[allow(dead_code)]
    #[serde(default)]
    lib: String,
    #[serde(default)]
    symbol: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StructMeta {
    size: u32,
    #[allow(dead_code)]
    #[serde(default)]
    align: u32,
    #[allow(dead_code)]
    #[serde(default)]
    packed: bool,
}

struct CAbiState {
    externs: HashMap<(String, String), ExternMeta>,
    structs: HashMap<String, StructMeta>,
    libraries: HashMap<String, Library>,
}

impl CAbiState {
    fn new() -> Self {
        Self {
            externs: HashMap::new(),
            structs: HashMap::new(),
            libraries: HashMap::new(),
        }
    }

    fn reset(&mut self) {
        self.externs.clear();
        self.structs.clear();
        self.libraries.clear();
    }
}

thread_local! {
    static C_ABI: RefCell<CAbiState> = RefCell::new(CAbiState::new());
}

fn with_abi<R>(f: impl FnOnce(&CAbiState) -> R) -> R {
    C_ABI.with(|c| f(&c.borrow()))
}

fn with_abi_mut<R>(f: impl FnOnce(&mut CAbiState) -> R) -> R {
    C_ABI.with(|c| f(&mut c.borrow_mut()))
}

/// Load sibling `.abi.json` `externs` for `@c` imports (mirrors GPU ABI attach).
pub fn attach_c_abi_from_wat_path(wat_path: &str) {
    let path = Path::new(wat_path);
    let abi_path = path.with_extension("abi.json");
    C_ABI.with(|c| {
        let mut st = c.borrow_mut();
        st.reset();
        let Ok(text) = std::fs::read_to_string(&abi_path) else {
            return;
        };
        let Ok(file) = serde_json::from_str::<AbiFile>(&text) else {
            return;
        };
        for ext in file.externs {
            if ext.kind == "c" || ext.module.starts_with("c/") {
                st.externs
                    .insert((ext.module.clone(), ext.field.clone()), ext);
            }
        }
        for (name, meta) in file.structs {
            st.structs.insert(name, meta);
        }
    });
}

/// Resolves and links every `c/<lib>` WASM import using ABI metadata + libffi trampolines.
pub fn link_c_ffi_imports(
    linker: &mut Linker<()>,
    module: &Module,
    search_roots: &[PathBuf],
) -> Result<()> {
    for import in module.imports() {
        let import_module = import.module();
        if !import_module.starts_with("c/") {
            continue;
        }
        let field = import.name();
        let ExternType::Func(wasm_ty) = import.ty() else {
            continue;
        };
        let meta = with_abi(|abi| {
            abi.externs
                .get(&(import_module.to_string(), field.to_string()))
                .cloned()
        });
        let lib_name = import_module.strip_prefix("c/").unwrap_or(import_module);
        let symbol = meta
            .as_ref()
            .map(|m| m.symbol.clone())
            .unwrap_or_else(|| field.to_string());
        let fn_ptr = resolve_symbol(lib_name, &symbol, search_roots)?;
        let param_tags = meta.as_ref().map(|m| m.params.clone()).unwrap_or_default();
        let result_tag = meta
            .as_ref()
            .map(|m| m.result.clone())
            .unwrap_or_else(|| "void".to_string());
        let wasm_ty = wasm_ty.clone();
        let import_module = import_module.to_string();
        let field = field.to_string();
        let fn_addr = fn_ptr as usize;
        linker.func_new(
            &import_module,
            &field,
            wasm_ty,
            move |mut caller, args, results| {
                invoke_c(
                    &mut caller,
                    fn_addr as *mut std::ffi::c_void,
                    &param_tags,
                    &result_tag,
                    args,
                    results,
                )
            },
        )?;
    }
    Ok(())
}

fn resolve_symbol(
    lib_name: &str,
    symbol: &str,
    search_roots: &[PathBuf],
) -> Result<*mut std::ffi::c_void> {
    with_abi_mut(|abi| -> Result<*mut std::ffi::c_void> {
        if !abi.libraries.contains_key(lib_name) {
            let path = find_library_path(lib_name, search_roots).ok_or_else(|| {
                Error::msg(format!(
                    "could not locate native library '{lib_name}' (searched {:?})",
                    search_roots
                ))
            })?;
            let lib = unsafe { Library::new(&path) }.map_err(|e| {
                Error::msg(format!("failed to load library '{}': {e}", path.display()))
            })?;
            abi.libraries.insert(lib_name.to_string(), lib);
        }
        let lib = abi
            .libraries
            .get(lib_name)
            .ok_or_else(|| Error::msg(format!("library '{lib_name}' not cached")))?;
        unsafe {
            let sym: libloading::Symbol<*mut std::ffi::c_void> =
                lib.get(symbol.as_bytes()).map_err(|e| {
                    Error::msg(format!(
                        "symbol '{symbol}' not found in library '{lib_name}': {e}"
                    ))
                })?;
            Ok(*sym)
        }
    })
}

fn find_library_path(lib_name: &str, search_roots: &[PathBuf]) -> Option<PathBuf> {
    let file_names = library_file_names(lib_name);
    // 1. Program-local `native/` folder or the WAT's directory (dev / packaged binary).
    for root in search_roots {
        for name in &file_names {
            let native = root.join("native").join(name);
            if native.exists() {
                return Some(native);
            }
            let direct = root.join(name);
            if direct.exists() {
                return Some(direct);
            }
        }
    }
    // 2. CWD (may differ from the WAT's parent when `dream run` is invoked elsewhere).
    for name in &file_names {
        let path = PathBuf::from(name);
        if path.exists() {
            return Some(path);
        }
    }
    // 3. Common system library dirs by absolute path (skips `DYLD_LIBRARY_PATH` / `LD_LIBRARY_PATH`
    // shadowing, and gives a stable answer even on hosts without the loader configured).
    for dir in system_library_dirs() {
        for name in &file_names {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    // 4. Last resort: hand a bare `libX.dylib`/`X.dll`/... to libloading and let the OS loader
    // walk its own search path (`DYLD_FALLBACK_LIBRARY_PATH`, `LD_LIBRARY_PATH`, system dirs).
    for name in &file_names {
        let candidate = PathBuf::from(name);
        if unsafe { Library::new(&candidate) }.is_ok() {
            return Some(candidate);
        }
    }
    None
}

/// OS-standard directories `find_library_path` walks after program-local + CWD lookups have
/// failed. Kept small on purpose: order is Homebrew/macOS-standard first on darwin, then
/// distro-standard on linux, then Windows-typical. Missing directories are silently skipped.
fn system_library_dirs() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        vec![
            PathBuf::from("/opt/homebrew/lib"),
            PathBuf::from("/usr/local/lib"),
            PathBuf::from("/opt/local/lib"),
            PathBuf::from("/usr/lib"),
        ]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        vec![
            PathBuf::from("/usr/local/lib"),
            PathBuf::from("/usr/lib/x86_64-linux-gnu"),
            PathBuf::from("/usr/lib/aarch64-linux-gnu"),
            PathBuf::from("/usr/lib64"),
            PathBuf::from("/usr/lib"),
            PathBuf::from("/lib"),
        ]
    }
    #[cfg(target_os = "windows")]
    {
        let mut dirs = Vec::new();
        if let Ok(win) = std::env::var("WINDIR") {
            dirs.push(PathBuf::from(win).join("System32"));
        } else {
            dirs.push(PathBuf::from("C:\\Windows\\System32"));
        }
        dirs
    }
}

fn library_file_names(lib_name: &str) -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        vec![
            format!("{lib_name}.dll"),
            format!("lib{lib_name}.dll"),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            format!("lib{lib_name}.dylib"),
            format!("lib{lib_name}.a"),
            format!("{lib_name}.dylib"),
        ]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        vec![
            format!("lib{lib_name}.so"),
            format!("lib{lib_name}.a"),
            format!("{lib_name}.so"),
        ]
    }
}

fn ffi_arg_type(tag: &str) -> Type {
    match tag {
        "int" | "bool" | "byte" => Type::i32(),
        "long" => Type::i64(),
        "float" => Type::f32(),
        "double" => Type::f64(),
        _ => Type::pointer(),
    }
}

fn ffi_return_type(tag: &str) -> Type {
    match tag {
        "void" => Type::void(),
        "int" | "bool" | "byte" => Type::i32(),
        "long" => Type::i64(),
        "float" => Type::f32(),
        "double" => Type::f64(),
        _ => Type::pointer(),
    }
}

fn invoke_c(
    caller: &mut Caller<'_, ()>,
    fn_ptr: *mut std::ffi::c_void,
    param_tags: &[String],
    result_tag: &str,
    wasm_args: &[Val],
    wasm_results: &mut [Val],
) -> Result<()> {
    if param_tags.is_empty() && !wasm_args.is_empty() {
        return invoke_c_scalar_fallback(fn_ptr, wasm_args, wasm_results);
    }

    let memory = required_memory(caller)?;
    let arg_types: Vec<Type> = param_tags.iter().map(|t| ffi_arg_type(t)).collect();
    let ret_type = ffi_return_type(result_tag);
    let cif = Cif::new(arg_types.clone(), ret_type);

    let mut c_strings: Vec<CString> = Vec::new();
    let mut wide_bufs: Vec<Vec<u16>> = Vec::new();
    let mut out_i32: i32 = 0;
    let mut out_i64: i64 = 0;
    // Per-param scratch buffers for `struct_ptr:` / `out_struct:`. Kept alive for the whole
    // trampoline call so libffi always sees a live pointer (never realloc'd once we hand it
    // out). Populated with the guest bytes on the way in, written back on the way out for
    // `out_struct:`.
    let mut struct_scratch: HashMap<usize, Vec<u8>> = HashMap::new();
    let mut c_args: Vec<ArgSlot> = Vec::with_capacity(param_tags.len());

    // Pre-lookup struct sizes so borrows don't overlap with the mutable scratch map below.
    let struct_sizes: HashMap<String, u32> = with_abi(|abi| {
        abi.structs
            .iter()
            .map(|(k, v)| (k.clone(), v.size))
            .collect()
    });

    for (i, tag) in param_tags.iter().enumerate() {
        let wasm_val = wasm_args.get(i).copied().unwrap_or(Val::I32(0));
        match tag.as_str() {
            "int" | "bool" | "byte" => c_args.push(ArgSlot::I32(wasm_val.i32().unwrap_or(0))),
            "long" => c_args.push(ArgSlot::I64(wasm_val.i64().unwrap_or(0))),
            "float" => c_args.push(ArgSlot::F32(wasm_val.f32().unwrap_or(0.0))),
            "double" => c_args.push(ArgSlot::F64(wasm_val.f64().unwrap_or(0.0))),
            "string" => {
                let ptr = wasm_val.i32().unwrap_or(0);
                let s = super::memory::read_string_from_memory(&memory, ptr);
                let cstr = CString::new(s).map_err(|_| Error::msg("string contains NUL"))?;
                c_args.push(ArgSlot::Ptr(cstr.as_ptr() as usize));
                c_strings.push(cstr);
            }
            "string_utf16" => {
                let ptr = wasm_val.i32().unwrap_or(0);
                let s = super::memory::read_string_from_memory(&memory, ptr);
                let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
                let p = wide.as_ptr() as usize;
                c_args.push(ArgSlot::Ptr(p));
                wide_bufs.push(wide);
            }
            "bytes" => {
                c_args.push(ArgSlot::Ptr(wasm_val.i32().unwrap_or(0) as usize));
            }
            t if t.starts_with("struct_ptr:") => {
                let name = &t["struct_ptr:".len()..];
                let guest_ptr = wasm_val.i32().unwrap_or(0);
                let ptr = struct_arg_pointer(
                    i,
                    guest_ptr,
                    name,
                    &struct_sizes,
                    &memory,
                    &mut struct_scratch,
                );
                c_args.push(ArgSlot::Ptr(ptr));
            }
            t if t.starts_with("out_struct:") => {
                let name = &t["out_struct:".len()..];
                let guest_ptr = wasm_val.i32().unwrap_or(0);
                let ptr = struct_arg_pointer(
                    i,
                    guest_ptr,
                    name,
                    &struct_sizes,
                    &memory,
                    &mut struct_scratch,
                );
                c_args.push(ArgSlot::Ptr(ptr));
            }
            "fn" => {
                let p = wasm_val.i32().unwrap_or(0);
                if p == 0 {
                    c_args.push(ArgSlot::Ptr(0));
                } else {
                    // Real callback trampolines would require a per-call libffi Closure that
                    // re-enters wasm; until that lands, refuse the call so failures are loud
                    // rather than silently passing a bogus pointer.
                    return Err(Error::msg(
                        "C callbacks: non-null Dream `fun` values are not yet supported for `@c` externs (pass null / 0 for now)",
                    ));
                }
            }
            t if t.starts_with("out_int") => {
                out_i32 = 0;
                c_args.push(ArgSlot::Ptr(&mut out_i32 as *mut i32 as usize));
            }
            t if t.starts_with("out_long") => {
                out_i64 = 0;
                c_args.push(ArgSlot::Ptr(&mut out_i64 as *mut i64 as usize));
            }
            _ => c_args.push(ArgSlot::Ptr(wasm_val.i32().unwrap_or(0) as usize)),
        }
    }

    let ret = unsafe { call_cif(&cif, fn_ptr, &c_args, result_tag)? };

    for (i, tag) in param_tags.iter().enumerate() {
        let wasm_val = wasm_args.get(i).copied().unwrap_or(Val::I32(0));
        if tag.starts_with("out_int") {
            let guest_ptr = wasm_val.i32().unwrap_or(0);
            if guest_ptr > 0 {
                let data = shared_bytes_mut(&memory);
                let base = guest_ptr as usize;
                if base + 4 <= data.len() {
                    data[base..base + 4].copy_from_slice(&out_i32.to_le_bytes());
                }
            }
        } else if tag.starts_with("out_long") {
            let guest_ptr = wasm_val.i32().unwrap_or(0);
            if guest_ptr > 0 {
                let data = shared_bytes_mut(&memory);
                let base = guest_ptr as usize;
                if base + 8 <= data.len() {
                    data[base..base + 8].copy_from_slice(&out_i64.to_le_bytes());
                }
            }
        } else if tag.starts_with("out_struct:") {
            let guest_ptr = wasm_val.i32().unwrap_or(0);
            if guest_ptr > 0 {
                if let Some(buf) = struct_scratch.get(&i) {
                    let data = shared_bytes_mut(&memory);
                    let base = guest_ptr as usize;
                    if base + buf.len() <= data.len() {
                        data[base..base + buf.len()].copy_from_slice(buf);
                    }
                }
            }
        }
    }

    if let Some(slot) = wasm_results.first_mut() {
        match result_tag {
            "void" => {}
            "int" | "bool" | "byte" => *slot = Val::I32(ret.i32),
            "long" => *slot = Val::I64(ret.i64),
            "float" => *slot = Val::F32(ret.f32.to_bits()),
            "double" => *slot = Val::F64(ret.f64.to_bits()),
            _ => *slot = Val::I32(ret.ptr as i32),
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct RetSlot {
    i32: i32,
    i64: i64,
    f32: f32,
    f64: f64,
    ptr: usize,
}

#[derive(Clone, Copy)]
enum ArgSlot {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    Ptr(usize),
}

unsafe fn call_cif(
    cif: &Cif,
    fn_ptr: *mut std::ffi::c_void,
    args: &[ArgSlot],
    result_tag: &str,
) -> Result<RetSlot> {
    let code = CodePtr(fn_ptr);
    let mut i32_buf = Vec::new();
    let mut i64_buf = Vec::new();
    let mut f32_buf = Vec::new();
    let mut f64_buf = Vec::new();
    let mut ptr_buf = Vec::new();
    let mut ffi_args = Vec::with_capacity(args.len());
    for slot in args {
        match *slot {
            ArgSlot::I32(v) => {
                i32_buf.push(v);
                ffi_args.push(arg(i32_buf.last().unwrap()));
            }
            ArgSlot::I64(v) => {
                i64_buf.push(v);
                ffi_args.push(arg(i64_buf.last().unwrap()));
            }
            ArgSlot::F32(v) => {
                f32_buf.push(v);
                ffi_args.push(arg(f32_buf.last().unwrap()));
            }
            ArgSlot::F64(v) => {
                f64_buf.push(v);
                ffi_args.push(arg(f64_buf.last().unwrap()));
            }
            ArgSlot::Ptr(v) => {
                ptr_buf.push(v as *mut std::ffi::c_void);
                ffi_args.push(arg(ptr_buf.last().unwrap()));
            }
        }
    }
    Ok(match result_tag {
        "void" => {
            cif.call::<()>(code, &ffi_args);
            RetSlot {
                i32: 0,
                i64: 0,
                f32: 0.0,
                f64: 0.0,
                ptr: 0,
            }
        }
        "int" | "bool" | "byte" => {
            let v: i32 = cif.call(code, &ffi_args);
            slot_from_i32(v)
        }
        "long" => {
            let v: i64 = cif.call(code, &ffi_args);
            slot_from_i64(v)
        }
        "float" => {
            let v: f32 = cif.call(code, &ffi_args);
            slot_from_f32(v)
        }
        "double" => {
            let v: f64 = cif.call(code, &ffi_args);
            slot_from_f64(v)
        }
        _ => {
            let v: usize = cif.call(code, &ffi_args);
            slot_from_ptr(v)
        }
    })
}

fn slot_from_i32(value: i32) -> RetSlot {
    RetSlot {
        i32: value,
        i64: value as i64,
        f32: value as f32,
        f64: value as f64,
        ptr: value as usize,
    }
}

fn slot_from_i64(value: i64) -> RetSlot {
    RetSlot {
        i32: value as i32,
        i64: value,
        f32: value as f32,
        f64: value as f64,
        ptr: value as usize,
    }
}

fn slot_from_f32(value: f32) -> RetSlot {
    RetSlot {
        i32: value as i32,
        i64: value as i64,
        f32: value,
        f64: value as f64,
        ptr: value.to_bits() as usize,
    }
}

fn slot_from_f64(value: f64) -> RetSlot {
    RetSlot {
        i32: value as i32,
        i64: value as i64,
        f32: value as f32,
        f64: value,
        ptr: value.to_bits() as usize,
    }
}

fn slot_from_ptr(value: usize) -> RetSlot {
    RetSlot {
        i32: value as i32,
        i64: value as i64,
        f32: value as f32,
        f64: value as f64,
        ptr: value,
    }
}

/// Copies a guest struct at `guest_ptr` into a fresh scratch buffer sized from the ABI `structs`
/// map, hands back a pointer into that scratch (kept alive via `scratch[param_index]` for the
/// full trampoline call). For unknown / unsized structs, falls back to a bare zero pointer so the
/// C side receives NULL rather than an unrelated address (still safer than dereferencing a raw
/// guest offset that maps to unrelated wasm host state).
fn struct_arg_pointer(
    param_index: usize,
    guest_ptr: i32,
    struct_name: &str,
    struct_sizes: &HashMap<String, u32>,
    memory: &wasmtime::SharedMemory,
    scratch: &mut HashMap<usize, Vec<u8>>,
) -> usize {
    if guest_ptr <= 0 {
        return 0;
    }
    let size = match struct_sizes.get(struct_name) {
        Some(&s) if s > 0 => s as usize,
        // Unknown/zero-sized: hand the C side NULL so bugs are loud rather than corrupting memory.
        _ => return 0,
    };
    let data = shared_bytes(memory);
    let base = guest_ptr as usize;
    if base + size > data.len() {
        return 0;
    }
    let mut buf = vec![0u8; size];
    buf.copy_from_slice(&data[base..base + size]);
    scratch.insert(param_index, buf);
    // Re-read the pointer *after* moving the buffer into the map: the Vec's heap allocation
    // stays put on move, but reading it out of `scratch` avoids relying on that guarantee.
    scratch
        .get(&param_index)
        .map(|b| b.as_ptr() as usize)
        .unwrap_or(0)
}

fn invoke_c_scalar_fallback(
    fn_ptr: *mut std::ffi::c_void,
    wasm_args: &[Val],
    wasm_results: &mut [Val],
) -> Result<()> {
    let arg_types: Vec<Type> = wasm_args
        .iter()
        .map(|v| match v {
            Val::I32(_) => Type::i32(),
            Val::I64(_) => Type::i64(),
            Val::F32(_) => Type::f32(),
            Val::F64(_) => Type::f64(),
            _ => Type::i32(),
        })
        .collect();
    let ret_type = if wasm_results.is_empty() {
        Type::void()
    } else {
        match wasm_results[0] {
            Val::I64(_) => Type::i64(),
            Val::F32(_) => Type::f32(),
            Val::F64(_) => Type::f64(),
            _ => Type::i32(),
        }
    };
    let cif = Cif::new(arg_types, ret_type);
    let slots: Vec<ArgSlot> = wasm_args
        .iter()
        .map(|v| match v {
            Val::I32(n) => ArgSlot::I32(*n),
            Val::I64(n) => ArgSlot::I64(*n),
            Val::F32(n) => ArgSlot::F32(f32::from_bits(*n)),
            Val::F64(n) => ArgSlot::F64(f64::from_bits(*n)),
            _ => ArgSlot::I32(0),
        })
        .collect();
    let ret = unsafe { call_cif(&cif, fn_ptr, &slots, "long")? };
    if let Some(slot) = wasm_results.first_mut() {
        match slot {
            Val::I64(_) => *slot = Val::I64(ret.i64),
            Val::F32(_) => *slot = Val::F32(ret.f32.to_bits()),
            Val::F64(_) => *slot = Val::F64(ret.f64.to_bits()),
            _ => *slot = Val::I32(ret.i32),
        }
    }
    Ok(())
}
