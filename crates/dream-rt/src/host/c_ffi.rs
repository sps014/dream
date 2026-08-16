//! `@c` libffi host for LLVM-native binaries (same ABI as wasmtime `execution/host/c_ffi.rs`).

use crate::guest;
use libffi::low::ffi_cif;
use libffi::middle::{arg, Cif, Closure, CodePtr, Type};
use libloading::Library;
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::{c_void, CStr, CString};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

extern "C" {
    fn dream_call_guest(
        idx: i32,
        a0: i64,
        a1: i64,
        a2: i64,
        a3: i64,
        a4: i64,
        a5: i64,
        a6: i64,
        a7: i64,
    ) -> i64;
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
    structs: HashMap<String, u32>,
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
}

static ABI: Mutex<Option<CAbiState>> = Mutex::new(None);

fn abi() -> std::sync::MutexGuard<'static, Option<CAbiState>> {
    ABI.lock().expect("c-ffi abi")
}

pub fn attach_from_path(path: &str) {
    let p = Path::new(path);
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let abi_path = if name.ends_with(".abi.json") {
        p.to_path_buf()
    } else {
        p.with_extension("abi.json")
    };
    let Ok(text) = std::fs::read_to_string(&abi_path) else {
        *abi() = Some(CAbiState::new());
        return;
    };
    attach_from_json(&text);
}

pub fn attach_from_json(text: &str) {
    let mut st = CAbiState::new();
    if let Ok(file) = serde_json::from_str::<AbiFile>(text) {
        for ext in file.externs {
            if ext.kind == "c" || ext.module.starts_with("c/") {
                st.externs
                    .insert((ext.module.clone(), ext.field.clone()), ext);
            }
        }
        for (name, meta) in file.structs {
            st.structs.insert(name, meta.size);
        }
    }
    *abi() = Some(st);
}

pub fn load_from_env() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if let Ok(p) = std::env::var("DREAM_ABI_JSON") {
            attach_from_path(&p);
        } else if let Ok(exe) = std::env::current_exe() {
            attach_from_path(exe.to_str().unwrap_or(""));
        }
    });
}

enum ArgSlot {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    Ptr(usize),
}

struct CbData {
    funcidx: i32,
    arg_kinds: Vec<&'static str>,
    ret_kind: &'static str,
}

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

unsafe extern "C" fn dream_callback_entry(
    _cif: &ffi_cif,
    result: &mut c_void,
    args: *const *const c_void,
    userdata: &CbData,
) {
    let mut packed = [0i64; 8];
    for (i, kind) in userdata.arg_kinds.iter().enumerate().take(8) {
        let arg_ptr = unsafe { *args.add(i) };
        packed[i] = match *kind {
            "i32" => i64::from(unsafe { *(arg_ptr as *const i32) }),
            "f32" => f32::to_bits(unsafe { *(arg_ptr as *const f32) }) as i64,
            "f64" => f64::to_bits(unsafe { *(arg_ptr as *const f64) }) as i64,
            _ => {
                if std::mem::size_of::<usize>() == 8 {
                    unsafe { *(arg_ptr as *const i64) }
                } else {
                    i64::from(unsafe { *(arg_ptr as *const u32) })
                }
            }
        };
    }
    let r = unsafe {
        dream_call_guest(
            userdata.funcidx,
            packed[0],
            packed[1],
            packed[2],
            packed[3],
            packed[4],
            packed[5],
            packed[6],
            packed[7],
        )
    };
    unsafe {
        match userdata.ret_kind {
            "void" => {}
            "i64" => *(result as *mut c_void as *mut i64) = r,
            "f32" => *(result as *mut c_void as *mut f32) = f32::from_bits(r as u32),
            "f64" => *(result as *mut c_void as *mut f64) = f64::from_bits(r as u64),
            _ => *(result as *mut c_void as *mut i32) = r as i32,
        }
    }
}

fn cstr_from_i8(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() }
}

fn find_library(lib_name: &str) -> Option<PathBuf> {
    let names = {
        #[cfg(target_os = "macos")]
        {
            vec![
                format!("lib{lib_name}.dylib"),
                format!("{lib_name}.dylib"),
            ]
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            vec![format!("lib{lib_name}.so"), format!("{lib_name}.so")]
        }
        #[cfg(target_os = "windows")]
        {
            vec![format!("{lib_name}.dll")]
        }
    };
    let mut roots = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            roots.push(d.to_path_buf());
            roots.push(d.join("native"));
        }
    }
    roots.push(PathBuf::from("."));
    roots.push(PathBuf::from("native"));
    #[cfg(target_os = "macos")]
    {
        roots.push(PathBuf::from("/opt/homebrew/lib"));
        roots.push(PathBuf::from("/usr/local/lib"));
        roots.push(PathBuf::from("/usr/lib"));
    }
    for root in roots {
        for n in &names {
            let c = root.join(n);
            if c.exists() {
                return Some(c);
            }
        }
    }
    names.first().map(PathBuf::from)
}

unsafe fn call_cif(cif: &Cif, fn_ptr: *mut c_void, slots: &[ArgSlot], result_tag: &str) -> i64 {
    let mut i32s = Vec::new();
    let mut i64s = Vec::new();
    let mut f32s = Vec::new();
    let mut f64s = Vec::new();
    let mut ptrs = Vec::new();
    let mut args = Vec::new();
    for s in slots {
        match *s {
            ArgSlot::I32(v) => {
                i32s.push(v);
                args.push(arg(i32s.last().unwrap()));
            }
            ArgSlot::I64(v) => {
                i64s.push(v);
                args.push(arg(i64s.last().unwrap()));
            }
            ArgSlot::F32(v) => {
                f32s.push(v);
                args.push(arg(f32s.last().unwrap()));
            }
            ArgSlot::F64(v) => {
                f64s.push(v);
                args.push(arg(f64s.last().unwrap()));
            }
            ArgSlot::Ptr(v) => {
                ptrs.push(v);
                args.push(arg(ptrs.last().unwrap()));
            }
        }
    }
    let code = CodePtr::from_ptr(fn_ptr);
    match result_tag {
        "void" => {
            cif.call::<()>(code, &args);
            0
        }
        "long" => cif.call::<i64>(code, &args),
        "float" => f32::to_bits(cif.call::<f32>(code, &args)) as i64,
        "double" => f64::to_bits(cif.call::<f64>(code, &args)) as i64,
        _ => cif.call::<i32>(code, &args) as i64,
    }
}

fn ffi_ty(tag: &str) -> Type {
    match tag {
        "int" | "bool" | "byte" => Type::i32(),
        "long" => Type::i64(),
        "float" => Type::f32(),
        "double" => Type::f64(),
        _ => Type::pointer(),
    }
}

fn ffi_ret(tag: &str) -> Type {
    match tag {
        "void" => Type::void(),
        "long" => Type::i64(),
        "float" => Type::f32(),
        "double" => Type::f64(),
        _ => Type::i32(),
    }
}

fn pack_load(pack: i32, i: usize) -> i64 {
    let off = pack.saturating_add((i * 8) as i32);
    match guest::copy_out(off, 8) {
        Some(b) if b.len() == 8 => i64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
        _ => 0,
    }
}

fn write_guest_i32(ptr: i32, v: i32) {
    guest::copy_in(ptr, &v.to_le_bytes());
}

fn write_guest_i64(ptr: i32, v: i64) {
    guest::copy_in(ptr, &v.to_le_bytes());
}

fn struct_scratch(
    guest_ptr: i32,
    struct_name: &str,
    sizes: &HashMap<String, u32>,
    scratch: &mut HashMap<usize, Vec<u8>>,
    param_index: usize,
) -> usize {
    if guest_ptr <= 0 {
        return 0;
    }
    let size = match sizes.get(struct_name) {
        Some(&s) if s > 0 => s as usize,
        _ => return 0,
    };
    let Some(buf) = guest::copy_out(guest_ptr, size) else {
        return 0;
    };
    scratch.insert(param_index, buf);
    scratch
        .get(&param_index)
        .map(|b| b.as_ptr() as usize)
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn dream_c_invoke(
    module: *const c_char,
    field: *const c_char,
    argc: i32,
    pack: i32,
) -> i64 {
    load_from_env();
    let module = cstr_from_i8(module);
    let field = cstr_from_i8(field);
    let lib_name = module.strip_prefix("c/").unwrap_or(&module).to_string();
    let (fn_ptr, param_tags, result_tag, struct_sizes) = {
        let mut guard = abi();
        let st = guard.get_or_insert_with(CAbiState::new);
        let meta = st.externs.get(&(module.clone(), field.clone())).cloned();
        let symbol = meta
            .as_ref()
            .map(|m| {
                if m.symbol.is_empty() {
                    field.clone()
                } else {
                    m.symbol.clone()
                }
            })
            .unwrap_or_else(|| field.clone());
        let param_tags = meta.as_ref().map(|m| m.params.clone()).unwrap_or_default();
        let result_tag = meta
            .as_ref()
            .map(|m| m.result.clone())
            .unwrap_or_else(|| "int".into());
        let struct_sizes = st.structs.clone();
        if !st.libraries.contains_key(&lib_name) {
            let path = match find_library(&lib_name) {
                Some(p) => p,
                None => {
                    eprintln!("dream c-ffi: library '{lib_name}' not found");
                    return 0;
                }
            };
            match unsafe { Library::new(&path) } {
                Ok(lib) => {
                    st.libraries.insert(lib_name.clone(), lib);
                }
                Err(e) => {
                    eprintln!("dream c-ffi: load {}: {e}", path.display());
                    return 0;
                }
            }
        }
        let lib = st.libraries.get(&lib_name).unwrap();
        let c_sym = CString::new(symbol.clone()).unwrap_or_default();
        let fn_ptr = {
            let sym: Result<libloading::Symbol<*mut c_void>, _> =
                unsafe { lib.get(c_sym.as_bytes_with_nul()) };
            match sym {
                Ok(s) => *s,
                Err(e) => {
                    eprintln!("dream c-ffi: symbol '{symbol}': {e}");
                    return 0;
                }
            }
        };
        (fn_ptr, param_tags, result_tag, struct_sizes)
    };

    let types: Vec<Type> = if param_tags.is_empty() {
        (0..argc).map(|_| Type::i32()).collect()
    } else {
        param_tags.iter().map(|t| ffi_ty(t)).collect()
    };
    let cif = Cif::new(types, ffi_ret(&result_tag));
    let mut c_strings: Vec<CString> = Vec::new();
    let mut wide_bufs: Vec<Vec<u16>> = Vec::new();
    let mut out_i32 = 0i32;
    let mut out_i64 = 0i64;
    let mut struct_bufs: HashMap<usize, Vec<u8>> = HashMap::new();
    let mut cb_keep: Vec<(Closure<'_>, Box<CbData>)> = Vec::new();
    let mut slots: Vec<ArgSlot> = Vec::new();
    let n = if param_tags.is_empty() {
        argc as usize
    } else {
        param_tags.len()
    };
    for i in 0..n {
        let raw = pack_load(pack, i);
        let tag = param_tags.get(i).map(|s| s.as_str()).unwrap_or("int");
        match tag {
            "int" | "bool" | "byte" => slots.push(ArgSlot::I32(raw as i32)),
            "long" => slots.push(ArgSlot::I64(raw)),
            "float" => slots.push(ArgSlot::F32(f32::from_bits(raw as u32))),
            "double" => slots.push(ArgSlot::F64(f64::from_bits(raw as u64))),
            "string" => {
                let s = guest::read_string(raw as i32);
                let cstr = CString::new(s).unwrap_or_default();
                slots.push(ArgSlot::Ptr(cstr.as_ptr() as usize));
                c_strings.push(cstr);
            }
            "string_utf16" => {
                let s = guest::read_string(raw as i32);
                let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
                slots.push(ArgSlot::Ptr(wide.as_ptr() as usize));
                wide_bufs.push(wide);
            }
            t if t.starts_with("out_int") => {
                slots.push(ArgSlot::Ptr(&mut out_i32 as *mut i32 as usize));
            }
            t if t.starts_with("out_long") => {
                slots.push(ArgSlot::Ptr(&mut out_i64 as *mut i64 as usize));
            }
            t if t.starts_with("struct_ptr:") => {
                let name = &t["struct_ptr:".len()..];
                let p = struct_scratch(raw as i32, name, &struct_sizes, &mut struct_bufs, i);
                slots.push(ArgSlot::Ptr(p));
            }
            t if t.starts_with("out_struct:") => {
                let name = &t["out_struct:".len()..];
                let p = struct_scratch(raw as i32, name, &struct_sizes, &mut struct_bufs, i);
                slots.push(ArgSlot::Ptr(p));
            }
            t if t == "fn" || t.starts_with("fn:") => {
                let idx = raw as i32;
                if idx == 0 {
                    slots.push(ArgSlot::Ptr(0));
                } else {
                    let (arg_kinds, ret_kind) = parse_fn_tag(t);
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
                    let data = Box::new(CbData {
                        funcidx: idx,
                        arg_kinds,
                        ret_kind,
                    });
                    let data_ptr = data.as_ref() as *const CbData;
                    let cif_cb = Cif::new(cif_args, cif_ret);
                    let closure = Closure::new(cif_cb, dream_callback_entry, unsafe { &*data_ptr });
                    let code = *closure.code_ptr() as *mut c_void as usize;
                    slots.push(ArgSlot::Ptr(code));
                    cb_keep.push((closure, data));
                }
            }
            "bytes" => {
                let p = raw as i32;
                let host = if p <= 0 {
                    0usize
                } else {
                    unsafe { crate::dream_heap_base().add((p + 4) as usize) as usize }
                };
                slots.push(ArgSlot::Ptr(host));
            }
            _ => slots.push(ArgSlot::Ptr(raw as usize)),
        }
    }
    let _keep_wide = &wide_bufs;
    let ret = unsafe { call_cif(&cif, fn_ptr, &slots, &result_tag) };
    drop(cb_keep);
    for (i, tag) in param_tags.iter().enumerate() {
        let raw = pack_load(pack, i) as i32;
        if tag.starts_with("out_int") {
            write_guest_i32(raw, out_i32);
        } else if tag.starts_with("out_long") {
            write_guest_i64(raw, out_i64);
        } else if tag.starts_with("out_struct:") {
            if let Some(buf) = struct_bufs.get(&i) {
                guest::copy_in(raw, buf);
            }
        }
    }
    ret
}
