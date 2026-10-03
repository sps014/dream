//! Reverse trampolines let C call Dream: one per C signature for `NativeCallback` (the closure
//! travels as `user_data`), and one per `(target, signature)` for a plain `fun` whose signature
//! needs conversion (C has no `user_data` to carry the target, so the trampoline *is* the target).
//! Each is emitted here as `<symbol>__body` in Dream's register types; the C shim defines
//! `<symbol>` itself with the real C prototype, so C never sees an un-narrowed scalar.

use super::super::fx::V;
use super::super::ir::{FnTy, Ty};
use super::super::lcx::{FnSig, Lcx};
use super::super::types::{fn_ptr_sig, ll_ty};
use super::glue;
use crate::backend::shared::glue::release_sym;
use crate::backend::shared::panic_msgs;
use crate::{Global, MirFunction};
use dream_abi::c_abi::shim::reverse_body_name;
use dream_hir::CShape;
use dream_types::{TyKind, TypeId};
use indexmap::IndexMap;

/// A reverse trampoline to emit: C-callable, entering Dream through `target`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) enum Reverse {
    /// `NativeCallback<F>`: `user_data` is the callback object, `fun_ty` is `F`.
    Callback {
        fun_ty: TypeId,
        user_data_last: bool,
    },
    /// A fixed Dream function (named function or non-capturing lambda) of type `fun_ty`.
    Direct { symbol: String, fun_ty: TypeId },
}

impl Reverse {
    /// The C-callable adapter, defined by the shim.
    pub(super) fn symbol(&self) -> String {
        match self {
            Reverse::Callback {
                fun_ty,
                user_data_last,
            } => format!(
                "dream_ctramp_{}{}",
                fun_ty.0,
                if *user_data_last { "_udl" } else { "" }
            ),
            Reverse::Direct { symbol, fun_ty } => format!("{symbol}__c_{}", fun_ty.0),
        }
    }

    /// The Dream-side body the adapter calls.
    pub(super) fn body(&self) -> String {
        reverse_body_name(&self.symbol())
    }

    pub(super) fn fun_ty(&self) -> TypeId {
        match self {
            Reverse::Callback { fun_ty, .. } | Reverse::Direct { fun_ty, .. } => *fun_ty,
        }
    }

    /// Where `user_data` sits among the C parameters, for a `NativeCallback`.
    pub(super) fn user_data_at(&self, arity: usize) -> Option<usize> {
        match self {
            Reverse::Callback {
                user_data_last: true,
                ..
            } => Some(arity),
            Reverse::Callback { .. } => Some(0),
            Reverse::Direct { .. } => None,
        }
    }
}

pub(super) fn fun_parts(l: &Lcx<'_>, fun_ty: TypeId) -> (Vec<TypeId>, TypeId) {
    match l.interner.kind(fun_ty) {
        TyKind::Func(params, ret) => (params.clone(), *ret),
        other => crate::internal_error!("C callback of non-function type {other:?}"),
    }
}

/// `Option<T>`'s `T`, or `ty` itself.
fn unwrap_option(l: &Lcx<'_>, ty: TypeId) -> TypeId {
    l.cx.nunion(ty)
        .and_then(|u| u.variant("Some"))
        .and_then(|v| v.fields.first())
        .map(|f| f.ty)
        .unwrap_or(ty)
}

/// The `fun` type a callback-carrying parameter of type `ty` (`fun`, `NativeCallback<fun>`, or an
/// `Option` of either) calls.
pub(super) fn callback_fun_ty(l: &Lcx<'_>, ty: TypeId) -> TypeId {
    let inner = unwrap_option(l, ty);
    match l.interner.kind(inner) {
        TyKind::Func(..) => inner,
        TyKind::Struct(_, args) if args.len() == 1 => args[0],
        other => crate::internal_error!("C callback parameter of type {other:?}"),
    }
}

/// Address-taken, synchronous functions of exactly `fun_ty`'s signature: the targets a plain
/// `fun` argument of that type can name.
pub(super) fn direct_targets<'m>(l: &Lcx<'m>, fun_ty: TypeId) -> Vec<&'m MirFunction> {
    let (params, ret) = fun_parts(l, fun_ty);
    let taken = crate::passes::funcbox_abi::address_taken(l.mir);
    l.mir
        .functions
        .iter()
        .filter(|f| {
            !f.is_async
                && f.ret == ret
                && f.params.len() == params.len()
                && f.params
                    .iter()
                    .zip(&params)
                    .all(|(p, t)| f.local_ty(*p) == *t)
                && taken.contains(&(f.def, f.instance.clone()))
        })
        .collect()
}

/// The body's type in one Dream register type per C parameter.
fn body_sig(l: &Lcx<'_>, rev: &Reverse, shapes: &[CShape], ret: &CShape) -> (Ty, Vec<Ty>) {
    let (params, ret_ty) = fun_parts(l, rev.fun_ty());
    let carrier = |s: &CShape, t: TypeId| match s {
        CShape::Void => Ty::Void,
        CShape::Scalar(_) => ll_ty(l.interner, t, &l.h(), &l.word()),
        _ => Ty::Ptr,
    };
    let mut c: Vec<Ty> = shapes
        .iter()
        .zip(&params)
        .map(|(s, t)| carrier(s, *t))
        .collect();
    if let Some(at) = rev.user_data_at(c.len()) {
        c.insert(at, Ty::Ptr);
    }
    (carrier(ret, ret_ty), c)
}

/// Every reverse trampoline the module's `@c` imports need, with the callback signature shapes.
pub(super) fn reverse_trampolines(l: &Lcx<'_>) -> IndexMap<Reverse, (Vec<CShape>, CShape)> {
    let mut out = IndexMap::new();
    for imp in l.mir.imports.iter().filter(|i| !i.c_params.is_empty()) {
        for (i, s) in imp.c_params.iter().enumerate() {
            let fun_ty = || callback_fun_ty(l, imp.params[i]);
            match s {
                CShape::Callback {
                    params,
                    ret,
                    user_data_last,
                    ..
                } => {
                    let rev = Reverse::Callback {
                        fun_ty: fun_ty(),
                        user_data_last: *user_data_last,
                    };
                    out.insert(rev, (params.clone(), (**ret).clone()));
                }
                CShape::Func { params, ret, .. } if s.needs_wrapper() => {
                    let fun_ty = fun_ty();
                    for f in direct_targets(l, fun_ty) {
                        let symbol = l.abi_sym(&l.user_fn(f));
                        out.insert(
                            Reverse::Direct { symbol, fun_ty },
                            (params.clone(), (**ret).clone()),
                        );
                    }
                }
                _ => {}
            }
        }
    }
    out
}

pub(super) fn register_reverse(l: &mut Lcx<'_>) {
    for (rev, (shapes, ret)) in reverse_trampolines(l) {
        let (r, ps) = body_sig(l, &rev, &shapes, &ret);
        l.own_shim_callee(&rev.body(), FnSig::plain(FnTy::new(r.clone(), ps.clone())));
        l.host(&rev.symbol(), FnSig::plain(FnTy::new(r, ps)));
    }
}

pub(super) fn emit_reverse(l: &mut Lcx<'_>) {
    for (rev, (shapes, ret)) in reverse_trampolines(l) {
        reverse_trampoline(l, &rev, &shapes, &ret);
    }
}

fn reverse_trampoline(l: &mut Lcx<'_>, rev: &Reverse, shapes: &[CShape], ret: &CShape) {
    let fun_ty = rev.fun_ty();
    let (params, _) = fun_parts(l, fun_ty);
    let mut fx = glue(l, &rev.body());
    let ud_at = rev.user_data_at(shapes.len());
    match ud_at {
        Some(at) => {
            let object = fx.arg(at);
            fx.call("dream_callback_check", &[object]);
        }
        None => {
            fx.call("dream_callback_enter", &[]);
        }
    }
    let first = usize::from(ud_at == Some(0));
    let mut args = Vec::with_capacity(params.len());
    let mut owned: Vec<(V, TypeId)> = Vec::new();
    for (i, (s, ty)) in shapes.iter().zip(&params).enumerate() {
        let c = fx.arg(first + i);
        let what = panic_msgs::c_callback_arg_what(i);
        let v = match s {
            CShape::Str { optional } => {
                let v = fx.c_string(&c, *optional, &what);
                owned.push((v.clone(), *ty));
                v
            }
            CShape::Ptr { optional: false } => {
                let layout = fx.l.cx.mir.layouts.target;
                let slot = fx.alloca_bytes(layout.ptr_size as u64, layout.ptr_align);
                let raw = V::u(fx.conv(&c, &fx.l.word()));
                fx.store_ty(&fx.l.word(), &slot, &raw, layout.ptr_align);
                fx.as_ref(&V::s(slot))
            }
            CShape::Ptr { optional: true } => {
                let v = fx.option_cptr_new(*ty, &c);
                owned.push((v.clone(), *ty));
                v
            }
            _ => c,
        };
        args.push(v);
    }
    let h = fx.h();
    let fp = match (rev, ud_at) {
        (Reverse::Callback { .. }, Some(at)) => {
            let obj = fx.arg(at);
            let class_ty = callback_class_ty(fx.l, fun_ty);
            let off =
                fx.l.cx
                    .nstruct(class_ty)
                    .and_then(|s| s.fields.first())
                    .map_or(0, |f| f.offset);
            let at = fx.addr(&obj, off as i64);
            let boxed = fx.load_ty(h.clone(), &at, fx.l.cx.mir.layouts.target.ptr_align, true);
            let env = fx.call_v("dream_funcbox_env", std::slice::from_ref(&boxed));
            fx.write_global(Global(0), &env);
            let idx = fx.call_v("dream_funcbox_funcidx", &[boxed]);
            fx.ft_entry(&idx)
        }
        (Reverse::Direct { symbol, .. }, _) => fx.l.fn_ref(symbol),
        (Reverse::Callback { .. }, None) => {
            crate::internal_error!("NativeCallback trampoline without user_data")
        }
    };
    let sig = fn_ptr_sig(fx.interner, fun_ty, &h, &fx.l.word());
    let coerced = fx.coerce_args(&sig, &args);
    let r = fx.call_ptr(&fp, &sig, coerced);
    for (v, ty) in owned {
        let sym = release_sym(&fx.l.cx, ty);
        fx.call(&sym, &[v]);
    }
    match (r, ret) {
        (Some(r), CShape::Ptr { .. }) => {
            let boxed = V::u(r);
            let bp = fx.ptr(&boxed);
            let raw = fx.load_ty(fx.l.word(), &bp, fx.l.cx.mir.layouts.target.ptr_align, true);
            fx.call("dream_free", &[boxed]);
            let p = fx.ptr(&raw);
            fx.w.ret(Some(&p));
        }
        (Some(r), CShape::Scalar(_)) => {
            let t = fx.w.ret.clone();
            let r = fx.conv(&V::s(r), &t);
            fx.w.ret(Some(&r));
        }
        _ => fx.w.ret(None),
    }
    fx.finish();
}

/// The `NativeCallback<F>` class type for `F`, found among the imports' parameter types.
fn callback_class_ty(l: &Lcx<'_>, fun_ty: TypeId) -> TypeId {
    l.mir
        .imports
        .iter()
        .flat_map(|imp| imp.params.iter().zip(&imp.c_params))
        .filter(|(_, s)| matches!(s, CShape::Callback { .. }))
        .map(|(t, _)| unwrap_option(l, *t))
        .find(|t| matches!(l.interner.kind(*t), TyKind::Struct(_, args) if args.first() == Some(&fun_ty)))
        .unwrap_or_else(|| crate::internal_error!("NativeCallback class for a C callback"))
}
