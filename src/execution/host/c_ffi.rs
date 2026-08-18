//! Native C FFI host: resolves `@c("lib", "symbol")` imports via libloading + libffi.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{c_void, CString};
use std::path::{Path, PathBuf};

use libffi::low::ffi_cif;
use libffi::middle::{arg, Cif, Closure, CodePtr, Type};
use libloading::Library;
use serde::Deserialize;
use wasmtime::*;

use super::memory::{decode_string, with_guest_bytes, with_guest_bytes_mut};

thread_local! {
    // Set for the duration of a native C call so Dream `fun` callbacks can re-enter the
    // current wasmtime Caller. Only valid while blocked inside `ffi_call`.
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
    #[serde(default)]
    symbol: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StructMeta {
    size: u32,
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

/// Load `@c` extern / struct metadata from `.abi.json` text (used by `dream run` and packed
/// `dream-runner` embeds).
pub fn attach_c_abi_from_json(text: &str) {
    C_ABI.with(|c| {
        let mut st = c.borrow_mut();
        st.reset();
        let Ok(file) = serde_json::from_str::<AbiFile>(text) else {
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

/// Load sibling `.abi.json` `externs` for `@c` imports (mirrors GPU ABI attach).
pub fn attach_c_abi_from_wat_path(wat_path: &str) {
    let path = Path::new(wat_path);
    let abi_path = path.with_extension("abi.json");
    let Ok(text) = std::fs::read_to_string(&abi_path) else {
        C_ABI.with(|c| c.borrow_mut().reset());
        return;
    };
    attach_c_abi_from_json(&text);
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
            let path = resolve_library_path(lib_name, search_roots).ok_or_else(|| {
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

fn resolve_library_path(lib_name: &str, search_roots: &[PathBuf]) -> Option<PathBuf> {
    let mut roots = Vec::new();
    for root in search_roots {
        let mut cur = Some(root.clone());
        while let Some(dir) = cur {
            if !roots.iter().any(|r| r == &dir) {
                roots.push(dir.clone());
            }
            cur = dir.parent().map(|p| p.to_path_buf());
        }
    }
    if let Some(found) = super::c_link::find_library_path(lib_name, &roots) {
        return Some(found);
    }
    for name in super::c_link::library_file_names(lib_name) {
        let candidate = PathBuf::from(name);
        if unsafe { Library::new(&candidate) }.is_ok() {
            return Some(candidate);
        }
    }
    None
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

/// Parse `fn` or `fn:i64,i32,i64,i64:i32` into (arg_kinds, ret_kind).
/// Bare `fn` defaults to the sqlite3_exec callback shape `(void*, int, char**, char**) -> int`.
fn parse_fn_tag(tag: &str) -> (Vec<&'static str>, &'static str) {
    let rest = tag.strip_prefix("fn").unwrap_or(tag);
    let rest = rest.strip_prefix(':').unwrap_or(rest);
    if rest.is_empty() {
        return (vec!["ptr", "i32", "ptr", "ptr"], "i32");
    }
    let (args_part, ret_part) = rest.split_once(':').unwrap_or((rest, "i32"));
    let args: Vec<&'static str> = args_part
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| match s.trim() {
            "int" | "bool" | "byte" | "i32" => "i32",
            "long" | "i64" => "i64",
            "float" | "f32" => "f32",
            "double" | "f64" => "f64",
            "ptr" | "pointer" | "string" => "ptr",
            _ => "ptr",
        })
        .collect();
    let ret = match ret_part.trim() {
        "void" => "void",
        "long" | "i64" => "i64",
        "float" | "f32" => "f32",
        "double" | "f64" => "f64",
        _ => "i32",
    };
    (args, ret)
}

/// libffi entry: C calls here, we re-enter the active wasmtime Caller and invoke the Dream fun.
unsafe extern "C" fn dream_callback_entry(
    _cif: &ffi_cif,
    result: &mut c_void,
    args: *const *const c_void,
    userdata: &DreamCbData,
) {
    let caller_ptr = ACTIVE_CALLER.with(|c| c.get());
    if caller_ptr.is_null() {
        return;
    }
    let caller: &mut Caller<'_, ()> = unsafe { &mut *caller_ptr };

    let mut wasm_args: Vec<Val> = Vec::with_capacity(userdata.arg_kinds.len());
    for (i, kind) in userdata.arg_kinds.iter().enumerate() {
        let arg_ptr = unsafe { *args.add(i) };
        let v = match *kind {
            "i32" => Val::I32(unsafe { *(arg_ptr as *const i32) }),
            "i64" | "ptr" => {
                // Pointers and i64: read as usize-width then widen to i64.
                let width = std::mem::size_of::<usize>();
                if width == 8 {
                    Val::I64(unsafe { *(arg_ptr as *const i64) })
                } else {
                    Val::I64(unsafe { *(arg_ptr as *const u32) } as i64)
                }
            }
            "f32" => Val::F32(unsafe { *(arg_ptr as *const f32) }.to_bits()),
            "f64" => Val::F64(unsafe { *(arg_ptr as *const f64) }.to_bits()),
            _ => Val::I64(0),
        };
        wasm_args.push(v);
    }

    let mut results = match userdata.ret_kind {
        "void" => vec![],
        "i64" => vec![Val::I64(0)],
        "f32" => vec![Val::F32(0)],
        "f64" => vec![Val::F64(0)],
        _ => vec![Val::I32(0)],
    };

    if let Err(e) = call_dream_funcref(caller, userdata.funcidx, &wasm_args, &mut results) {
        eprintln!("dream c-ffi callback error: {e}");
        return;
    }

    if userdata.ret_kind != "void" && !results.is_empty() {
        match userdata.ret_kind {
            "i64" => unsafe {
                *(result as *mut c_void as *mut i64) = results[0].unwrap_i64();
            },
            "f32" => unsafe {
                *(result as *mut c_void as *mut f32) = results[0].unwrap_f32();
            },
            "f64" => unsafe {
                *(result as *mut c_void as *mut f64) = results[0].unwrap_f64();
            },
            _ => unsafe {
                *(result as *mut c_void as *mut i32) = results[0].unwrap_i32();
            },
        }
    }
}

fn call_dream_funcref(
    caller: &mut Caller<'_, ()>,
    funcidx: u32,
    args: &[Val],
    results: &mut [Val],
) -> Result<()> {
    let table = caller
        .get_export("__indirect_function_table")
        .and_then(|e| e.into_table())
        .ok_or_else(|| Error::msg("module missing __indirect_function_table export"))?;
    let entry = table
        .get(&mut *caller, u64::from(funcidx))
        .ok_or_else(|| Error::msg(format!("funcref table index {funcidx} out of bounds")))?;
    let func = match entry {
        Ref::Func(Some(f)) => f,
        Ref::Func(None) => {
            return Err(Error::msg("null funcref for Dream callback"));
        }
        _ => {
            return Err(Error::msg("callback table entry is not a funcref"));
        }
    };
    func.call(&mut *caller, args, results)
        .map_err(|e| Error::msg(format!("Dream callback call failed: {e}")))?;
    Ok(())
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

    let guest = with_guest_bytes(caller, |d| d.to_vec())?;
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
    // Keep closures + userdata alive across `ffi_call` (C may invoke them).
    let mut cb_closures: Vec<Closure<'_>> = Vec::new();
    let mut cb_data_owners: Vec<Box<DreamCbData>> = Vec::new();

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
                let s = decode_string(&guest, ptr);
                let cstr = CString::new(s).map_err(|_| Error::msg("string contains NUL"))?;
                c_args.push(ArgSlot::Ptr(cstr.as_ptr() as usize));
                c_strings.push(cstr);
            }
            "string_utf16" => {
                let ptr = wasm_val.i32().unwrap_or(0);
                let s = decode_string(&guest, ptr);
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
                    &guest,
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
                    &guest,
                    &mut struct_scratch,
                );
                c_args.push(ArgSlot::Ptr(ptr));
            }
            t if t == "fn" || t.starts_with("fn:") => {
                let p = wasm_val.i32().unwrap_or(0);
                if p == 0 {
                    c_args.push(ArgSlot::Ptr(0));
                } else {
                    let (arg_kinds, ret_kind) = parse_fn_tag(t);
                    // Use pointer types for `ptr` so libffi matches the C ABI (not `i64`).
                    let cif_args: Vec<Type> = arg_kinds
                        .iter()
                        .map(|k| match *k {
                            "i32" => Type::i32(),
                            "i64" => Type::i64(),
                            "f32" => Type::f32(),
                            "f64" => Type::f64(),
                            _ => Type::pointer(),
                        })
                        .collect();
                    let cif_ret = match ret_kind {
                        "void" => Type::void(),
                        "i64" => Type::i64(),
                        "f32" => Type::f32(),
                        "f64" => Type::f64(),
                        _ => Type::i32(),
                    };
                    let cb_cif = Cif::new(cif_args, cif_ret);
                    // `p == 0` is the reserved null funcref (see MIR `func_table`); real callbacks
                    // start at index 1.
                    let data = Box::new(DreamCbData {
                        funcidx: p as u32,
                        arg_kinds,
                        ret_kind,
                    });
                    let data_ptr = data.as_ref() as *const DreamCbData;
                    cb_data_owners.push(data);
                    let data_ref = unsafe { &*data_ptr };
                    let closure = Closure::new(cb_cif, dream_callback_entry, data_ref);
                    let code = *closure.code_ptr() as *mut c_void as usize;
                    cb_closures.push(closure);
                    c_args.push(ArgSlot::Ptr(code));
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

    // Install Caller for nested Dream callbacks, then invoke C.
    // Lifetime transmute: the pointer is only dereferenced while `caller` is still borrowed
    // for this stack frame (blocked inside `ffi_call`).
    let caller_ptr =
        unsafe { std::mem::transmute::<*mut Caller<'_, ()>, *mut Caller<'static, ()>>(caller) };
    ACTIVE_CALLER.with(|c| c.set(caller_ptr));
    let ret = unsafe { call_cif(&cif, fn_ptr, &c_args, result_tag) };
    ACTIVE_CALLER.with(|c| c.set(std::ptr::null_mut()));
    // Drop closures before we continue using caller (they must outlive ffi_call only).
    drop(cb_closures);
    drop(cb_data_owners);
    let ret = ret?;

    with_guest_bytes_mut(caller, |data| {
        for (i, tag) in param_tags.iter().enumerate() {
            let wasm_val = wasm_args.get(i).copied().unwrap_or(Val::I32(0));
            if tag.starts_with("out_int") {
                let guest_ptr = wasm_val.i32().unwrap_or(0);
                if guest_ptr > 0 {
                    let base = guest_ptr as usize;
                    if base + 4 <= data.len() {
                        data[base..base + 4].copy_from_slice(&out_i32.to_le_bytes());
                    }
                }
            } else if tag.starts_with("out_long") {
                let guest_ptr = wasm_val.i32().unwrap_or(0);
                if guest_ptr > 0 {
                    let base = guest_ptr as usize;
                    if base + 8 <= data.len() {
                        data[base..base + 8].copy_from_slice(&out_i64.to_le_bytes());
                    }
                }
            } else if tag.starts_with("out_struct:") {
                let guest_ptr = wasm_val.i32().unwrap_or(0);
                if guest_ptr > 0 {
                    if let Some(buf) = struct_scratch.get(&i) {
                        let base = guest_ptr as usize;
                        if base + buf.len() <= data.len() {
                            data[base..base + buf.len()].copy_from_slice(buf);
                        }
                    }
                }
            }
        }
    })?;

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
    guest: &[u8],
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
    let base = guest_ptr as usize;
    if base + size > guest.len() {
        return 0;
    }
    let mut buf = vec![0u8; size];
    buf.copy_from_slice(&guest[base..base + size]);
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
