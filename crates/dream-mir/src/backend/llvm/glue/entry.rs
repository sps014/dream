//! The process entry: `dream_guest_entry` runs Dream `main` and computes the exit status (policy
//! in `shared::entry`), and native `main` wraps it with argument capture and the leak report.
//! On wasm32 the entry is exported as `main` and an async `main` hands its pending Future to the
//! JS host, which collects the status later through `__dream_main_report`.

use super::super::fx::{align_at, V};
use super::super::ir::{GlobalDef, Linkage, Ty, Value};
use super::super::lcx::Lcx;
use super::{glue, register};
use crate::backend::shared::entry::{
    entry_exit, err_variant, EntryExit, ERROR_PREFIX, RC_SLOT, STATUS_FN,
};
use crate::backend::shared::glue::release_sym;
use crate::backend::shared::protocol_names::to_string_fn;
use crate::MirFunction;
use dream_types::TypeId;

pub(in super::super) fn register_all(l: &mut Lcx<'_>) {
    let Some(main) = l
        .mir
        .functions
        .iter()
        .find(|f| f.name == crate::abi::ENTRY_FN)
    else {
        return;
    };
    if let EntryExit::Report(_) = entry_exit(main, &l.cx) {
        let h = l.h();
        register(l, STATUS_FN, Ty::I32, vec![h]);
    }
    register(l, crate::abi::GUEST_ENTRY_FN, Ty::I32, vec![]);
    l.export(crate::abi::GUEST_ENTRY_FN, crate::abi::ENTRY_FN);
    if l.cx.target.is_wasm32() {
        if entry_exit(main, &l.cx) != EntryExit::Void {
            let h = l.h();
            register(l, crate::abi::EXPORT_MAIN_REPORT, Ty::I32, vec![h]);
            l.export(
                crate::abi::EXPORT_MAIN_REPORT,
                crate::abi::EXPORT_MAIN_REPORT,
            );
        }
    } else {
        register(l, "main", Ty::I32, vec![Ty::I32, Ty::Ptr]);
    }
}

pub(in super::super) fn emit_all(l: &mut Lcx<'_>) {
    let mir = l.mir;
    let Some(main) = mir
        .functions
        .iter()
        .find(|f| f.name == crate::abi::ENTRY_FN)
    else {
        return;
    };
    let exit = entry_exit(main, &l.cx);
    if exit != EntryExit::Void {
        l.global(
            RC_SLOT,
            GlobalDef {
                linkage: Linkage::Internal,
                thread_local: false,
                constant: false,
                unnamed_addr: false,
                ty: Ty::I32,
                init: Some("0".into()),
                align: 4,
            },
        );
    }
    if let EntryExit::Report(ty) = exit {
        status_fn(l, ty);
    }
    guest_entry(l, main, exit);
    if l.cx.target.is_wasm32() {
        main_report(l, main, exit);
    } else {
        native_main(l);
    }
}

fn status_fn(l: &mut Lcx<'_>, ty: TypeId) {
    let err = err_variant(&l.cx, ty);
    let mut fx = glue(l, STATUS_FN);
    let v = fx.arg(0);
    let rc = match err {
        Some((disc, payload_ty)) => {
            let d = fx.union_discriminant(ty, &v);
            let d = fx.conv(&d, &Ty::I32);
            let is_err = fx.w.icmp("eq", &d, &Value::i32(disc as i64));
            let rc = fx.w.select(&is_err, &Value::i32(1), &Value::i32(0));
            fx.if_then(&is_err, |fx| {
                let prefix = fx.str_v(ERROR_PREFIX);
                fx.call("print_err_string", &[prefix]);
                let payload = fx.union_field(ty, disc, 0, &v);
                let conv = to_string_fn(&fx.l.cx, payload_ty);
                if conv.is_empty() {
                    fx.call("print_err_string", &[payload]);
                } else {
                    let t = fx.call_v(&conv, &[payload]);
                    fx.call("print_err_string", std::slice::from_ref(&t));
                    fx.call("dream_release", &[t]);
                }
                fx.call("print_err_char", &[V::i32(10)]);
            });
            rc
        }
        None => Value::i32(0),
    };
    let rel = release_sym(&fx.l.cx, ty);
    fx.call(&rel, &[v]);
    fx.w.ret(Some(&rc));
    fx.finish();
}

fn guest_entry(l: &mut Lcx<'_>, main: &MirFunction, exit: EntryExit) {
    let main_name = l.user_fn(main);
    let async_n = l.mir.polls.len();
    let uses_defer = l.mir.uses_defer;
    let wasm = l.cx.target.is_wasm32();
    let mut fx = glue(l, crate::abi::GUEST_ENTRY_FN);
    fx.call("dream_runtime_init", &[]);
    let args = if main.params.is_empty() {
        vec![]
    } else {
        let ps = fx.l.cx.target.abi().ptr_size as i64;
        vec![fx.call_v("dream_array_new", &[V::i32(0), V::i32(ps)])]
    };
    let value_ty = if exit == EntryExit::Code {
        Ty::I32
    } else {
        fx.h()
    };
    let r = fx.call(&main_name, &args);
    if main.is_async {
        let mf = r
            .clone()
            .unwrap_or_else(|| crate::internal_error!("async main returned void"));
        fx.call("dream_start", &[mf]);
    }
    if async_n > 0 {
        fx.call("dream_run_loop", &[]);
    }
    if uses_defer {
        fx.call("dream_defer_drain_all", &[]);
    }
    // An async `main` has settled by now on native; on wasm32 the host's loop has not run yet.
    if exit != EntryExit::Void && !(wasm && main.is_async) {
        let value = match (&r, main.is_async) {
            (Some(mf), true) => settled_value(&mut fx, mf, &value_ty, exit),
            (Some(mv), false) => mv.clone(),
            (None, _) => crate::internal_error!("main returns a status but its call is void"),
        };
        store_status(&mut fx, exit, value);
    }
    if wasm {
        let ret = match (&r, main.is_async) {
            (Some(mf), true) => fx.conv(mf, &Ty::I32),
            _ => Value::i32(0),
        };
        fx.w.ret(Some(&ret));
        fx.finish();
        return;
    }
    if main.is_async {
        if let Some(mf) = &r {
            fx.call("dream_release", std::slice::from_ref(mf));
        }
    }
    fx.call("dream_drop_globals", &[]);
    if exit == EntryExit::Void {
        fx.w.ret(Some(&Value::i32(0)));
    } else {
        let rc = fx.w.load(Ty::I32, &Value::global(RC_SLOT), 4, &[]);
        fx.w.ret(Some(&rc));
    }
    fx.finish();
}

fn settled_value(fx: &mut super::super::fx::Fx<'_, '_>, fut: &V, ty: &Ty, exit: EntryExit) -> V {
    let off = fx.l.cx.target.abi().future.result as i64;
    let at = fx.addr(fut, off);
    fx.load_ty(ty.clone(), &at, align_at(ty, off), exit != EntryExit::Code)
}

fn store_status(fx: &mut super::super::fx::Fx<'_, '_>, exit: EntryExit, value: V) {
    let status = match exit {
        EntryExit::Void => V::i32(0),
        EntryExit::Code => V::s(fx.conv(&value, &Ty::I32)),
        EntryExit::Report(_) => fx.call_v(STATUS_FN, &[value]),
    };
    let s = fx.conv(&status, &Ty::I32);
    fx.w.store(&s, &Value::global(RC_SLOT), 4, &[]);
}

/// wasm32: the host calls this once `main` finished. An async `main` passes its settled Future
/// (null when it never produced one); a sync one passes null and reads back the entry's status.
fn main_report(l: &mut Lcx<'_>, main: &MirFunction, exit: EntryExit) {
    if exit == EntryExit::Void {
        return;
    }
    let mut fx = glue(l, crate::abi::EXPORT_MAIN_REPORT);
    if main.is_async {
        let fut = fx.arg(0);
        let some = fx.truthy(&fut);
        let value_ty = if exit == EntryExit::Code {
            Ty::I32
        } else {
            fx.h()
        };
        fx.if_then(&some, |fx| {
            let v = settled_value(fx, &fut, &value_ty, exit);
            store_status(fx, exit, v);
        });
    }
    let rc = fx.w.load(Ty::I32, &Value::global(RC_SLOT), 4, &[]);
    fx.w.ret(Some(&rc));
    fx.finish();
}

fn native_main(l: &mut Lcx<'_>) {
    let always = l.cx.leak_checks as i64;
    let mut fx = glue(l, "main");
    let (argc, argv) = (fx.arg(0), fx.arg(1));
    fx.call("dream_process_capture_args", &[argc, argv]);
    let rc = fx.call_v(crate::abi::GUEST_ENTRY_FN, &[]);
    fx.call("dream_llvm_leak_report", &[V::i32(always)]);
    fx.w.ret(Some(&rc.v));
    fx.finish();
}
