//! Typed ownership for runtime-created futures and combinator result copies.
use super::{glue, register};
use super::super::{fx::V, ir::{Ty, Value}, lcx::Lcx};
use crate::backend::shared::{abi_types::elem_size, glue::{release_sym, retain_sym}};
use dream_types::{TypeId, TyKind, PrimTy};

pub(in super::super) fn info(l: &mut Lcx<'_>, result: TypeId) -> String {
    let name = format!("dream_future_info_{}", result.0);
    if !l.future_infos.insert(result) { return name; }
    let visit = format!("dream_future_visit_{}", result.0);
    let clear = format!("dream_future_clear_{}", result.0);
    for symbol in [&visit, &clear] { register(l, symbol, Ty::Void, vec![l.h()]); }
    let mut fx = glue(l, &visit);
    let owner = fx.arg(0);
    let children = fx.addr(&owner, fx.l.cx.target.abi().future.children as i64);
    let child = fx.load_ty(fx.h(), &children, 8, true);
    fx.call("dream_visit_edge", &[child]);
    let at = fx.addr(&owner, fx.l.cx.target.abi().future.result as i64);
    let value = fx.load_ty(fx.word(), &at, 8, true);
    if fx.is_value(result) {
        let value = fx.as_ref(&value);
        let nz = fx.truthy(&value);
        fx.if_then(&nz, |fx| fx.visit_refs(result, &value));
    } else if fx.is_rc(result) { fx.call("dream_visit_edge", &[value]); }
    fx.w.ret(None); fx.finish();
    let mut fx = glue(l, &clear);
    let owner = fx.arg(0);
    let children = fx.addr(&owner, fx.l.cx.target.abi().future.children as i64);
    let child = fx.load_ty(fx.h(), &children, 8, true);
    fx.store_ty(&fx.h(), &children, &V::s(Value::zero(fx.h())), 8);
    fx.call("dream_release_object", &[child]);
    let at = fx.addr(&owner, fx.l.cx.target.abi().future.result as i64);
    let value = fx.load_ty(fx.word(), &at, 8, true);
    fx.store_ty(&fx.word(), &at, &V::s(Value::zero(fx.word())), 8);
    if fx.is_value(result) {
        let value = fx.as_ref(&value);
        let nz = fx.truthy(&value);
        fx.if_then(&nz, |fx| { fx.clear_refs(result, &value); fx.call("dream_recycle", std::slice::from_ref(&value)); });
    } else if fx.is_rc(result) { fx.call(&release_sym(&fx.l.cx, result), &[value]); }
    fx.w.ret(None); fx.finish();
    super::ownership::descriptor(l, &name, &visit, None, &clear, true, false);
    name
}

pub(in super::super) fn copies(l: &mut Lcx<'_>, elem: TypeId) -> (String, String) {
    let copy = format!("dream_future_copy_{}", elem.0);
    let clone = format!("dream_future_clone_{}", elem.0);
    if !l.future_copies.insert(elem) { return (copy, clone); }
    register(l, &copy, Ty::Void, vec![l.h(), l.h()]);
    register(l, &clone, l.word(), vec![l.word()]);
    let mut fx = glue(l, &copy);
    let dest = fx.arg(0); let future = fx.arg(1);
    let wide = matches!(fx.interner.kind(elem), TyKind::Prim(PrimTy::Long | PrimTy::ULong | PrimTy::Float | PrimTy::Double));
    let offset = if wide { fx.l.cx.target.abi().future.wide } else { fx.l.cx.target.abi().future.result };
    let at = fx.addr(&future, offset as i64);
    let size = elem_size(&fx.l.cx, elem) as i64;
    let dp = fx.ptr(&dest);
    if fx.is_value(elem) {
        let value = fx.load_ty(fx.h(), &at, 8, true);
        let source = fx.ptr(&value);
        fx.memcpy(&dp, &source, &Value::i64(size));
        fx.value_refs(elem, &dest, true);
    } else {
        fx.memcpy(&dp, &at, &Value::i64(size));
        if fx.is_rc(elem) {
            let value = fx.load_ty(fx.h(), &dp, 4, true);
            fx.call(retain_sym(&fx.l.cx, elem), &[value]);
        }
    }
    fx.w.ret(None); fx.finish();
    let mut fx = glue(l, &clone);
    let value = fx.arg(0);
    if fx.is_value(elem) {
        let size = elem_size(&fx.l.cx, elem) as i64;
        let dest = fx.call_v("dream_malloc", &[V::i64(size), V::i32(0)]);
        let (dp, sp) = (fx.ptr(&dest), fx.ptr(&value));
        fx.memcpy(&dp, &sp, &Value::i64(size));
        fx.value_refs(elem, &dest, true);
        let result = fx.conv(&dest, &fx.word());
        fx.w.ret(Some(&result));
    } else {
        if fx.is_rc(elem) { fx.call(retain_sym(&fx.l.cx, elem), std::slice::from_ref(&value)); }
        fx.w.ret(Some(&value.v));
    }
    fx.finish();
    (copy, clone)
}

pub(super) fn bridge(l: &mut Lcx<'_>, name: &str, result: TypeId, params: &[(TypeId, i64)]) -> String {
    info(l, result);
    let visit = format!("{name}_visit");
    let clear = format!("{name}_clear");
    let metadata = format!("{name}_info");
    for symbol in [&visit, &clear] { register(l, symbol, Ty::Void, vec![l.h()]); }
    for (symbol, action) in [(&visit, "visit"), (&clear, "clear")] {
        let mut fx = glue(l, symbol);
        let owner = fx.arg(0);
        fx.call(&format!("dream_future_{action}_{}", result.0), std::slice::from_ref(&owner));
        for &(ty, offset) in params {
            let at = fx.addr(&owner, offset);
            let value = fx.load_ty(fx.h(), &at, 8, true);
            if action == "visit" {
                fx.call("dream_visit_edge", &[value]);
            } else {
                fx.store_ty(&fx.h(), &at, &V::s(Value::zero(fx.h())), 8);
                fx.call(&release_sym(&fx.l.cx, ty), &[value]);
            }
        }
        fx.w.ret(None); fx.finish();
    }
    super::ownership::descriptor(l, &metadata, &visit, None, &clear, true, false);
    metadata
}
