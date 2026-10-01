//! Module data and the functions that expose it: the string table, globals, the function and
//! drop tables, interface itables + dispatch trampolines, `dream_tag_name`, global teardown,
//! runtime init and the worker entry points.
//!
//! Every table is a constant initializer, so LLVM can see through indirect and interface calls.

use super::super::body::{drop_name, poll_name};
use super::super::fx::V;
use super::super::ir::{fmt, GlobalDef, Linkage, Ty, Value};
use super::super::lcx::Lcx;
use super::super::types::{abi_ll, ll_ty};
use super::{glue, register};
use crate::abi;
use crate::backend::shared::abi_types::{c_ident, elem_size, fn_sig, lazy_import_poll};
use crate::backend::shared::func_symbol;
use crate::backend::shared::glue::release_sym;
use std::collections::BTreeMap;

fn data_global(l: &mut Lcx<'_>, name: &str, ty: Ty, init: String, constant: bool, align: u32) {
    l.global(
        name,
        GlobalDef {
            linkage: Linkage::Internal,
            thread_local: false,
            constant,
            unnamed_addr: false,
            ty,
            init: Some(init),
            align,
        },
    );
}

/// Immortal string blocks use the target's heap header,
/// then `{ len, hash, units }`.
/// They are `constant`: the runtime never writes an immortal block (retain/release skip the
/// immortal rc, the hash is precomputed, in-place rebuilds require a unique owner), so LLVM may
/// fold a literal's length, slice flag and units.
pub(in super::super) fn emit_strings(l: &mut Lcx<'_>) {
    let strings: Vec<(String, String)> =
        l.cx.strings
            .iter()
            .map(|(s, sym)| (s.clone(), sym.clone()))
            .collect();
    for (s, sym) in strings {
        let units: Vec<u16> = s.encode_utf16().collect();
        let n = units.len().max(1) as u64;
        let arr = Ty::Array(n, Box::new(Ty::I16));
        let pad = if l.cx.target.is_wasm32() {
            ""
        } else {
            "i64 0, i32 0, i32 0, "
        };
        let size_ty = l.h();
        let mut fields = if l.cx.target.is_wasm32() {
            vec![Ty::I32; 5]
        } else {
            vec![
                size_ty.clone(),
                Ty::I64,
                Ty::I32,
                Ty::I32,
                Ty::I32,
                Ty::I32,
                Ty::I32,
                Ty::I32,
            ]
        };
        fields.push(arr.clone());
        let ty = Ty::Struct {
            packed: false,
            fields,
        };
        let elems = if units.is_empty() {
            "i16 0".to_string()
        } else {
            units
                .iter()
                .map(|u| format!("i16 {u}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let init = format!(
            "{{ {size_ty} 0, {pad}i32 {}, i32 {}, i32 {}, i32 {}, {arr} [{elems}] }}",
            abi::TAG_STRING,
            i32::MIN,
            units.len(),
            abi::string_hash(&units),
        );
        data_global(l, &format!("{sym}_blk"), ty, init, true, 8);
    }
}

pub(in super::super) fn emit_globals(l: &mut Lcx<'_>) {
    let globals: Vec<_> = l.mir.globals.iter().map(|g| (g.id, g.ty)).collect();
    for (id, ty) in globals {
        if id.0 == 0 && l.cx.target.is_wasm32() {
            continue;
        }
        if id.0 == 0 {
            l.global(
                "g0",
                GlobalDef {
                    linkage: Linkage::External,
                    thread_local: true,
                    constant: false,
                    unnamed_addr: false,
                    ty: Ty::I64,
                    init: Some("0".into()),
                    align: 8,
                },
            );
            continue;
        }
        if l.interner.is_value_type(ty) {
            let size = elem_size(&l.cx, ty).max(1) as u64;
            let vg = format!("__vg{}", id.0);
            data_global(l, &vg, Ty::bytes(size), "zeroinitializer".into(), false, 8);
            let h = l.h();
            let init = format!("ptrtoint (ptr {} to {h})", fmt::global(&vg));
            data_global(l, &format!("g{}", id.0), h, init, false, 8);
            continue;
        }
        let t = ll_ty(l.interner, ty, &l.h());
        data_global(
            l,
            &format!("g{}", id.0),
            t,
            "zeroinitializer".into(),
            false,
            8,
        );
    }
}

fn ptr_array(entries: &[Option<String>]) -> (Ty, String) {
    let ty = Ty::Array(entries.len() as u64, Box::new(Ty::Ptr));
    let body = entries
        .iter()
        .map(|e| match e {
            Some(name) => format!("ptr {}", fmt::global(name)),
            None => "ptr null".into(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    (ty, format!("[{body}]"))
}

pub(in super::super) fn register_all(l: &mut Lcx<'_>) {
    register(l, "dream_ft_get", Ty::Ptr, vec![Ty::I32]);
    register(l, "dream_fd_get", Ty::Ptr, vec![Ty::I32]);
    if !l.cx.target.is_wasm32() {
        register(l, "dream_tag_name", Ty::Ptr, vec![Ty::I32]);
    }
    register(l, "dream_drop_globals", Ty::Void, vec![]);
    register(l, "dream_runtime_init", Ty::Void, vec![]);
    let h = l.h();
    let worker = vec![Ty::I32, h.clone(), h.clone()];
    register(l, "dream_worker_invoke_raw", h.clone(), worker.clone());
    register(l, "dream_worker_invoke", h, worker);
    for (name, export) in [
        ("dream_ft_get", crate::abi::EXPORT_FT_GET),
        ("dream_drop_globals", crate::abi::EXPORT_DROP_GLOBALS),
        ("dream_runtime_init", crate::abi::EXPORT_RUNTIME_INIT),
        (
            "dream_worker_invoke_raw",
            crate::abi::EXPORT_WORKER_INVOKE_RAW,
        ),
        ("dream_worker_invoke", crate::abi::EXPORT_WORKER_INVOKE),
    ] {
        l.export(name, export);
    }
    for (td, (ret, params)) in dispatch_sigs(l) {
        let mut p = vec![Ty::Ptr];
        p.extend(params);
        register(l, &c_ident(&format!("__iface_dispatch_{td}")), ret, p);
    }
}

pub(in super::super) fn emit_all(l: &mut Lcx<'_>) {
    emit_ftables(l);
    if !l.cx.target.is_wasm32() {
        emit_tag_names(l);
    }
    emit_itables(l);
    emit_drop_globals(l);
    emit_runtime_init(l);
    emit_worker_invoke(l);
}

fn emit_ftables(l: &mut Lcx<'_>) {
    let n = l.cx.ftable_len();
    let mut ft: Vec<Option<String>> = vec![None; n];
    let mut fd: Vec<Option<String>> = vec![None; n];
    let fns = &l.mir.functions;
    for f in fns {
        ft[l.cx.func_index(f)] = Some(l.boxed_sym(&l.user_fn(f)));
    }
    for (async_i, f) in fns.iter().filter(|f| f.is_async).enumerate() {
        let i = fns.len() + 1 + async_i;
        ft[i] = Some(poll_name(l, f));
        fd[i] = Some(drop_name(l, f));
    }
    let base = l.cx.import_poll_base();
    for (i, poll) in l
        .mir
        .imports
        .iter()
        .filter_map(lazy_import_poll)
        .enumerate()
    {
        ft[base + i] = Some(poll);
    }
    ft[0] = None;
    let (ty, init) = ptr_array(&ft);
    data_global(l, "dream_ft", ty, init, true, 8);
    let (ty, init) = ptr_array(&fd);
    data_global(l, "dream_fd", ty, init, true, 8);
    for (name, table) in [("dream_ft_get", "dream_ft"), ("dream_fd_get", "dream_fd")] {
        let mut fx = glue(l, name);
        let i = fx.arg(0);
        let pos = fx.w.icmp("sgt", &i.v, &Value::i32(0));
        let below = fx.w.icmp("slt", &i.v, &Value::i32(n as i64));
        let ok = fx.w.bin("and", &pos, &below);
        fx.if_then(&ok, |fx| {
            let idx = fx.conv(&i, &Ty::I64);
            let ps = fx.l.cx.target.abi().ptr_size as i64;
            let off = fx.w.bin("mul", &idx, &Value::i64(ps));
            let at = fx.w.gep_i8(&Value::global(table), &off);
            let p = fx.w.load(Ty::Ptr, &at, ps as u32, &[]);
            fx.w.ret(Some(&p));
        });
        fx.w.ret(Some(&Value::null()));
        fx.finish();
    }
}

fn emit_tag_names(l: &mut Lcx<'_>) {
    let mut pairs: Vec<(i32, String)> =
        l.cx.tags
            .iter()
            .map(|(ty, tag)| {
                let name = l
                    .mir
                    .layouts
                    .structs
                    .get(ty)
                    .map(|x| x.name.as_str())
                    .or_else(|| l.mir.layouts.unions.get(ty).map(|x| x.name.as_str()))
                    .unwrap_or("object");
                (*tag, name.to_string())
            })
            .collect();
    pairs.sort();
    let mut arms: Vec<(i32, String)> = vec![
        (abi::TAG_FUTURE, "future".into()),
        (0, "untagged".into()),
        (abi::TAG_STRING, "string".into()),
        (abi::TAG_ARRAY, "array".into()),
        (abi::TAG_FUNCBOX, "funcbox".into()),
        (abi::TAG_CLOSURE_ENV, "closure_env".into()),
    ];
    for (t, n) in pairs {
        if !arms.iter().any(|(k, _)| *k == t) {
            arms.push((t, n));
        }
    }
    let mut cstrs: Vec<String> = Vec::new();
    let mut cstr = |l: &mut Lcx<'_>, s: &str| -> Value {
        let name = format!(".tn{}", cstrs.len());
        cstrs.push(name.clone());
        let bytes: Vec<u8> = s.bytes().chain(std::iter::once(0)).collect();
        l.global(
            &name,
            GlobalDef {
                linkage: Linkage::Private,
                thread_local: false,
                constant: true,
                unnamed_addr: true,
                ty: Ty::bytes(bytes.len() as u64),
                init: Some(fmt::c_string(&bytes)),
                align: 1,
            },
        );
        Value::global(name)
    };
    let names: Vec<(i32, Value)> = arms.iter().map(|(t, n)| (*t, cstr(l, n))).collect();
    let object = cstr(l, "object");
    let mut fx = glue(l, "dream_tag_name");
    let tag = fx.arg(0);
    let kind =
        fx.w.bin("and", &tag.v, &Value::i32(abi::TAG_VALUE_MASK as i64));
    let other = fx.w.new_block("other");
    let blocks: Vec<_> = names.iter().map(|_| fx.w.new_block("tag")).collect();
    let cases: Vec<_> = names
        .iter()
        .zip(&blocks)
        .map(|((t, _), b)| (*t as i128, *b))
        .collect();
    fx.w.switch(&kind, other, &cases);
    for ((_, v), b) in names.iter().zip(blocks) {
        fx.w.switch_to(b);
        fx.w.ret(Some(v));
    }
    fx.w.switch_to(other);
    fx.w.ret(Some(&object));
    fx.finish();
}

/// One trampoline per distinct method signature.
fn dispatch_sigs(l: &Lcx<'_>) -> BTreeMap<String, (Ty, Vec<Ty>)> {
    let mut out = BTreeMap::new();
    for iface in &l.mir.interfaces.interfaces {
        for slot in 0..iface.method_count {
            let (td, ret, params) = fn_sig(l.interner, iface.sigs[slot]);
            out.entry(td).or_insert_with(|| {
                let h = l.h();
                (
                    abi_ll(ret, &h),
                    params.into_iter().map(|p| abi_ll(p, &h)).collect(),
                )
            });
        }
    }
    out
}

fn emit_itables(l: &mut Lcx<'_>) {
    let ntags =
        l.cx.tags
            .values()
            .copied()
            .max()
            .unwrap_or(abi::TAG_STRUCT_BASE) as usize
            + 1;
    for (td, (ret, params)) in dispatch_sigs(l) {
        let name = c_ident(&format!("__iface_dispatch_{td}"));
        let mut fx = glue(l, &name);
        let itab = fx.arg(0);
        let this = fx.arg(1);
        let tag = fx.call_v("dream_object_tag", &[this]);
        let tag = fx.conv(&tag, &Ty::I32);
        let oob = fx.w.icmp("uge", &tag, &Value::i32(ntags as i64));
        fx.if_then(&oob, |fx| {
            fx.call("abort", &[]);
            if !fx.w.is_terminated() {
                fx.w.unreachable();
            }
        });
        let idx = fx.conv(&V::u(tag), &Ty::I64);
        let ps = fx.l.cx.target.abi().ptr_size as i64;
        let off = fx.w.bin("mul", &idx, &Value::i64(ps));
        let at = fx.w.gep_i8(&itab.v, &off);
        let f = fx.w.load(Ty::Ptr, &at, ps as u32, &[]);
        let missing = fx.w.icmp("eq", &f, &Value::null());
        fx.if_then(&missing, |fx| {
            fx.call("abort", &[]);
            if !fx.w.is_terminated() {
                fx.w.unreachable();
            }
        });
        let sig = super::super::lcx::FnSig::plain(super::super::ir::FnTy::new(
            ret.clone(),
            params.clone(),
        ));
        let args: Vec<Value> = (0..params.len()).map(|i| fx.w.param(i + 1)).collect();
        let r = fx.call_ptr(&f, &sig, args);
        match r {
            Some(r) => fx.w.ret(Some(&r)),
            None => fx.w.ret(None),
        }
        fx.finish();
    }
    let mut tables: Vec<Vec<Option<String>>> = Vec::new();
    let mut index: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for (iid, iface) in l.mir.interfaces.interfaces.iter().enumerate() {
        for slot in 0..iface.method_count {
            index.insert((iid, slot), tables.len());
            tables.push(vec![None; ntags]);
        }
    }
    for imp in &l.mir.interfaces.impls {
        let Some(tag) = l.cx.interface_tag(imp.class_ty) else {
            continue;
        };
        for (iid, symbols) in &imp.entries {
            for (slot, sym) in symbols.iter().enumerate() {
                let Some(f) = l.mir.functions.iter().find(|f| f.name == *sym) else {
                    continue;
                };
                let (Some(&t), true) = (index.get(&(*iid, slot)), (tag as usize) < ntags) else {
                    continue;
                };
                tables[t][tag as usize] = Some(l.boxed_sym(&c_ident(&func_symbol(f))));
            }
        }
    }
    for ((iid, slot), t) in index {
        let (ty, init) = ptr_array(&tables[t]);
        data_global(l, &format!("dream_iface_{iid}_{slot}"), ty, init, true, 8);
    }
}

fn emit_drop_globals(l: &mut Lcx<'_>) {
    let globals: Vec<_> = l.mir.globals.iter().map(|g| (g.id, g.ty)).collect();
    let mut fx = glue(l, "dream_drop_globals");
    for (id, ty) in globals {
        if id.0 == 0 {
            continue;
        }
        if fx.is_value(ty) {
            let v = fx.read_global(id);
            fx.value_refs(ty, &v, false);
            continue;
        }
        if !fx.is_rc(ty) {
            continue;
        }
        let v = fx.read_global(id);
        let rel = release_sym(&fx.l.cx, ty);
        fx.call(&rel, &[v]);
        let (t, _) = fx.global_ll(id);
        fx.write_global(id, &V::s(Value::zero(t)));
    }
    fx.w.ret(None);
    fx.finish();
}

fn emit_runtime_init(l: &mut Lcx<'_>) {
    data_global(l, "dream_rt_inited", Ty::I32, "0".into(), false, 4);
    let init = l
        .mir
        .functions
        .iter()
        .find(|f| f.name == crate::lower::INIT_FN_NAME)
        .map(|f| l.user_fn(f));
    let mut fx = glue(l, "dream_runtime_init");
    let flag = Value::global("dream_rt_inited");
    let done = fx.w.load(Ty::I32, &flag, 4, &[]);
    let done = fx.w.icmp("ne", &done, &Value::i32(0));
    fx.if_then(&done, |fx| fx.w.ret(None));
    fx.w.store(&Value::i32(1), &flag, 4, &[]);
    if fx.l.cx.target.is_wasm32() {
        fx.call("dream_heap_init", &[]);
    } else {
        fx.call("dream_thread_attach", &[]);
        let fns: Vec<V> = [
            "dream_string_alloc",
            "dream_array_new_shared",
            "dream_complete_foreign",
        ]
        .iter()
        .map(|n| V::s(fx.l.fn_ref(n)))
        .collect();
        fx.call("dream_host_bind", &fns);
    }
    if let Some(init) = init {
        fx.call(&init, &[]);
    }
    fx.w.ret(None);
    fx.finish();
}

fn emit_worker_invoke(l: &mut Lcx<'_>) {
    let has_env = l.mir.globals.iter().any(|g| g.id.0 == 0);
    let result_off = l.cx.target.abi().future.result as i64;
    let async_idx: Vec<usize> = l
        .mir
        .functions
        .iter()
        .filter(|f| f.is_async)
        .map(|f| l.cx.func_index(f))
        .collect();

    let wasm = l.cx.target.is_wasm32();
    let mut fx = glue(l, "dream_worker_invoke_raw");
    let (f, env, arg) = (fx.arg(0), fx.arg(1), fx.arg(2));
    if wasm {
        fx.call("dream_runtime_init", &[]);
    }
    let none = fx.w.icmp("sle", &f.v, &Value::i32(0));
    let h = fx.h();
    fx.if_then(&none, |fx| fx.w.ret(Some(&Value::zero(h.clone()))));
    if has_env {
        fx.write_global(crate::Global(0), &env);
    }
    let fp = fx.ft_entry(&f);
    let sig =
        super::super::lcx::FnSig::plain(super::super::ir::FnTy::new(h.clone(), vec![h.clone()]));
    let r = fx
        .call_ptr(&fp, &sig, vec![arg.v.clone()])
        .unwrap_or_else(|| crate::internal_error!("worker body returned void"));
    fx.call("dream_release", &[arg]);
    if wasm {
        // Worker bodies cross the string wire; a returned lazy Future is launched here so the JS
        // host's status polling observes progress.
        let r = V::u(r.clone());
        let tag = fx.call_v("dream_object_tag", std::slice::from_ref(&r));
        let tag = fx.conv(&tag, &Ty::I32);
        let fut =
            fx.w.icmp("eq", &tag, &Value::i32(crate::abi::TAG_FUTURE as i64));
        fx.if_then(&fut, |fx| {
            fx.call("dream_start", std::slice::from_ref(&r));
        });
    }
    fx.w.ret(Some(&r));
    fx.finish();

    let mut fx = glue(l, "dream_worker_invoke");
    let args = [fx.arg(0), fx.arg(1), fx.arg(2)];
    let result = fx.call_v("dream_worker_invoke_raw", &args);
    if !async_idx.is_empty() {
        let launch = fx.w.new_block("launch");
        let plain = fx.w.new_block("plain");
        let cases: Vec<_> = async_idx.iter().map(|i| (*i as i128, launch)).collect();
        fx.w.switch(&args[0].v, plain, &cases);
        fx.w.switch_to(launch);
        fx.call("dream_start", std::slice::from_ref(&result));
        fx.call("dream_run_loop", &[]);
        let at = fx.addr(&result, result_off);
        let settled = fx.load_ty(fx.h(), &at, 8, true);
        fx.call("dream_release", std::slice::from_ref(&result));
        fx.w.ret(Some(&settled.v));
        fx.w.switch_to(plain);
    }
    let r = fx.as_ref(&result);
    fx.w.ret(Some(&r.v));
    fx.finish();
}
