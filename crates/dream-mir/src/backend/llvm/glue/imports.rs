//! Extern imports: `@c` trampolines (Dream types → C ABI), host functions the runtime header does
//! not declare, and lazy async host bridges (stub + poll). On wasm32 every host function is a
//! wasm import named by its extern, `@c` imports do not exist, and an async bridge hands its future
//! to the JS host, which settles it through `__dream_resolve`.

use super::super::fx::V;
use super::super::ir::{FnTy, Ty, Value};
use super::super::lcx::{FnSig, Lcx};
use super::super::types::ll_ty;
use super::{c_marshal, c_reverse, glue, register};
use crate::backend::shared::abi_types::{
    import_call_name, import_host_name, is_c_import, native_header_declares,
};
use dream_abi::js_abi;
use dream_hir::HImport;
use dream_types::TyKind;

fn by_ref(imp: &HImport, i: usize) -> bool {
    imp.param_by_ref.get(i).copied().unwrap_or(false)
}

fn dream_ret(l: &Lcx<'_>, imp: &HImport) -> Ty {
    imp.ret
        .map(|t| ll_ty(l.interner, t, &l.h(), &l.word()))
        .unwrap_or(Ty::Void)
}

/// Host-side parameter types: a by-ref argument is the `dream_ptr` of the caller's storage.
fn host_params(l: &Lcx<'_>, imp: &HImport) -> Vec<Ty> {
    imp.params
        .iter()
        .enumerate()
        .map(|(i, t)| {
            if by_ref(imp, i) {
                l.h()
            } else {
                ll_ty(l.interner, *t, &l.h(), &l.word())
            }
        })
        .collect()
}

/// An async bridge's settled value type (`Future<T>`'s `T`; `void` and unknown settle as `int`).
fn async_host_ret(l: &Lcx<'_>, imp: &HImport) -> Ty {
    match imp.ret.map(|r| l.interner.kind(r)) {
        Some(TyKind::Struct(_, args)) => match args.first() {
            Some(t) if matches!(l.interner.kind(*t), TyKind::Void) => Ty::I32,
            Some(t) => ll_ty(l.interner, *t, &l.h(), &l.word()),
            None => Ty::I32,
        },
        _ => Ty::I32,
    }
}

/// The wasm32 import `(module, field)` of an extern.
fn wasm_import_names(imp: &HImport) -> (String, String) {
    let module = if imp.module.is_empty() {
        js_abi::HOST_MODULE
    } else {
        &imp.module
    };
    let field = if imp.field.is_empty() {
        &imp.name
    } else {
        &imp.field
    };
    (module.to_string(), field.to_string())
}

fn register_wasm(l: &mut Lcx<'_>, imports: &[HImport]) {
    let h = l.h();
    for (name, field) in [
        ("js_retain", js_abi::HOST_JS_RETAIN),
        ("js_release", js_abi::HOST_JS_RELEASE),
    ] {
        let sig = FnSig::plain(FnTy::new(Ty::Void, vec![Ty::I32]));
        l.wasm_import(name, sig, js_abi::HOST_MODULE, field);
    }
    for imp in imports {
        if is_c_import(imp)
            || imp.field == js_abi::HOST_JS_RETAIN
            || imp.field == js_abi::HOST_JS_RELEASE
        {
            continue;
        }
        let host = import_host_name(imp);
        let (module, field) = wasm_import_names(imp);
        let params = host_params(l, imp);
        if !imp.is_async {
            let sig = FnSig::plain(FnTy::new(dream_ret(l, imp), params));
            l.wasm_import(&host, sig, &module, &field);
            continue;
        }
        let mut hp = vec![h.clone()];
        hp.extend(params.iter().cloned());
        l.wasm_import(
            &host,
            FnSig::plain(FnTy::new(Ty::Void, hp)),
            &module,
            &field,
        );
        let name = import_call_name(imp);
        register(l, &name, h.clone(), params);
        register(l, &format!("{name}_poll"), Ty::I32, vec![h.clone()]);
    }
}

pub(in super::super) fn register_all(l: &mut Lcx<'_>) {
    let imports = l.mir.imports.clone();
    if l.cx.target.spec().capabilities.js_interop {
        register_wasm(l, &imports);
        return;
    }
    c_reverse::register_reverse(l);
    for imp in &imports {
        let host = import_host_name(imp);
        let name = import_call_name(imp);
        if is_c_import(imp) {
            c_marshal::register_import(l, imp);
            continue;
        }
        let params = host_params(l, imp);
        if !imp.is_async {
            if !native_header_declares(&host) {
                l.host(&host, FnSig::plain(FnTy::new(dream_ret(l, imp), params)));
            }
            continue;
        }
        if imp.async_host {
            let mut hp = vec![l.h()];
            hp.extend(params.iter().cloned());
            l.host(
                &format!("{host}Async"),
                FnSig::plain(FnTy::new(Ty::I32, hp)),
            );
        } else if !native_header_declares(&host) {
            l.host(
                &host,
                FnSig::plain(FnTy::new(async_host_ret(l, imp), params.clone())),
            );
        }
        let h = l.h();
        register(l, &name, h.clone(), params);
        register(l, &format!("{name}_poll"), Ty::I32, vec![h]);
    }
}

pub(in super::super) fn emit_all(l: &mut Lcx<'_>) {
    let imports = l.mir.imports.clone();
    if l.cx.target.spec().capabilities.c_interop {
        c_reverse::emit_reverse(l);
    }
    let mut poll_i = 0usize;
    for imp in &imports {
        if is_c_import(imp) {
            if l.cx.target.spec().capabilities.c_interop {
                c_marshal::trampoline(l, imp);
            }
        } else if imp.is_async {
            let idx = l.cx.import_poll_base() + poll_i;
            poll_i += 1;
            async_bridge(l, imp, idx);
        }
    }
}

/// Calling the extern builds an unstarted future carrying the arguments; its first poll performs
/// the host call. `@async_host` bridges hand the future to `<host>Async`, which completes it from
/// a foreign thread; other hosts block and complete inline.
fn async_bridge(l: &mut Lcx<'_>, imp: &HImport, poll_idx: usize) {
    let host = import_host_name(imp);
    let name = import_call_name(imp);
    let params = host_params(l, imp);
    let fut = l.cx.target.abi().future;
    let slot_base = fut.slots as i64;
    let frame_size = slot_base + params.len() as i64 * 8;

    let mut fx = glue(l, &name);
    let s = fx.call_v(
        "dream_new_future",
        &[
            V::i32(frame_size),
            V::i32(poll_idx as i64),
            V::i32(crate::abi::FUTURE_KIND_TASK as i64),
        ],
    );
    for (i, t) in params.iter().enumerate() {
        let a = fx.arg(i);
        let at = fx.addr(&s, slot_base + i as i64 * 8);
        fx.store_ty(t, &at, &a, 8);
    }
    let r = fx.as_ref(&s);
    fx.w.ret(Some(&r.v));
    fx.finish();

    let host_ret = async_host_ret(l, imp);
    let mut fx = glue(l, &format!("{name}_poll"));
    let s = fx.arg(0);
    let st_at = fx.addr(&s, fut.state as i64);
    let st = fx.load_ty(Ty::I32, &st_at, 4, false);
    let pending = fx.w.icmp("eq", &st.v, &Value::i32(0));
    fx.if_then(&pending, |fx| {
        let saved: Vec<V> = params
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let at = fx.addr(&s, slot_base + i as i64 * 8);
                fx.load_ty(t.clone(), &at, 8, *t == fx.h())
            })
            .collect();
        if fx.l.cx.target.spec().capabilities.js_interop {
            let mut a = vec![s.clone()];
            a.extend(saved);
            fx.call(&host, &a);
        } else if imp.async_host {
            fx.call("dream_foreign_work_begin", &[]);
            let mut a = vec![s.clone()];
            a.extend(saved);
            let deferred = fx.call_v(&format!("{host}Async"), &a);
            let sync =
                fx.w.icmp("eq", &deferred.v, &Value::zero(deferred.ty().clone()));
            fx.if_then(&sync, |fx| {
                fx.call("dream_foreign_work_end", &[]);
            });
        } else if host_ret.is_void() {
            fx.call(&host, &saved);
            fx.call("dream_async_complete", &[s.clone(), V::i32(0)]);
        } else {
            let r = fx.call(&host, &saved).unwrap_or_else(|| V::i32(0));
            let r = match r.ty() {
                Ty::F64 => V::u(fx.w.cast("bitcast", &r.v, Ty::I64)),
                Ty::F32 => {
                    let bits = V::u(fx.w.cast("bitcast", &r.v, Ty::I32));
                    fx.conv_v(&bits, &Ty::I64, true)
                }
                _ => fx.conv_v(&r, &Ty::I64, true),
            };
            fx.call("dream_async_complete", &[s.clone(), r]);
        }
        let st_at = fx.addr(&s, fut.state as i64);
        fx.store_ty(&Ty::I32, &st_at, &V::i32(1), 4);
    });
    fx.w.ret(Some(&Value::i32(0)));
    fx.finish();
}
