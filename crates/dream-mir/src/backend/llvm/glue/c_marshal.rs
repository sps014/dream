//! The `@c` boundary. A forward trampoline turns Dream arguments into C arguments by each
//! parameter's [`CShape`] (decided by the analyzer) and turns the C result back; reverse
//! trampolines let C call Dream: one per C signature for `NativeCallback` (the closure travels
//! as `user_data`), and one per `(target, signature)` for a plain `fun` whose signature needs
//! conversion (C has no `user_data` to carry the target, so the wrapper *is* the target).

use super::super::fx::{Fx, V};
use super::super::ir::{FnTy, Ty, Value};
use super::super::lcx::{FnSig, Lcx};
use super::super::types::{fn_ptr_sig, is_unsigned, ll_ty};
use super::{glue, register};
use crate::backend::shared::abi_types::{import_call_name, import_host_name};
use crate::backend::shared::glue::release_sym;
use crate::backend::shared::panic_msgs;
use crate::{Global, MirFunction};
use dream_hir::{CShape, HImport};
use dream_types::{TyKind, TypeId};
use indexmap::IndexMap;

/// A reverse trampoline to emit: C-callable, entering Dream through `target`.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Reverse {
    /// `NativeCallback<F>`: `user_data` is the callback object, `fun_ty` is `F`.
    Callback {
        fun_ty: TypeId,
        user_data_last: bool,
    },
    /// A fixed Dream function (named function or non-capturing lambda) of type `fun_ty`.
    Direct { symbol: String, fun_ty: TypeId },
}

impl Reverse {
    fn symbol(&self) -> String {
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
}

fn shape(imp: &HImport, i: usize) -> &CShape {
    imp.c_params.get(i).unwrap_or(&CShape::Scalar)
}

/// The C scalar type of a value of shape `s` and Dream type `ty`.
fn c_ty(l: &Lcx<'_>, s: &CShape, ty: TypeId) -> Ty {
    match s {
        CShape::Void => Ty::Void,
        CShape::Scalar => ll_ty(l.interner, ty, &l.h(), &l.word()),
        _ => Ty::Ptr,
    }
}

/// The C parameters one Dream argument expands to.
fn c_param_tys(l: &Lcx<'_>, imp: &HImport, i: usize) -> Vec<Ty> {
    match shape(imp, i) {
        CShape::Callback { .. } => vec![Ty::Ptr, Ty::Ptr],
        s => vec![c_ty(l, s, imp.params[i])],
    }
}

fn fun_parts(l: &Lcx<'_>, fun_ty: TypeId) -> (Vec<TypeId>, TypeId) {
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
fn callback_fun_ty(l: &Lcx<'_>, ty: TypeId) -> TypeId {
    let inner = unwrap_option(l, ty);
    match l.interner.kind(inner) {
        TyKind::Func(..) => inner,
        TyKind::Struct(_, args) if args.len() == 1 => args[0],
        other => crate::internal_error!("C callback parameter of type {other:?}"),
    }
}

/// Address-taken, synchronous functions of exactly `fun_ty`'s signature: the targets a plain
/// `fun` argument of that type can name.
fn direct_targets<'m>(l: &Lcx<'m>, fun_ty: TypeId) -> Vec<&'m MirFunction> {
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

fn reverse_sig(l: &Lcx<'_>, rev: &Reverse, shapes: &[CShape], ret: &CShape) -> (Ty, Vec<Ty>) {
    let fun_ty = match rev {
        Reverse::Callback { fun_ty, .. } | Reverse::Direct { fun_ty, .. } => *fun_ty,
    };
    let (params, ret_ty) = fun_parts(l, fun_ty);
    let mut c: Vec<Ty> = shapes
        .iter()
        .zip(&params)
        .map(|(s, t)| c_ty(l, s, *t))
        .collect();
    match rev {
        Reverse::Callback {
            user_data_last: true,
            ..
        } => c.push(Ty::Ptr),
        Reverse::Callback { .. } => c.insert(0, Ty::Ptr),
        Reverse::Direct { .. } => {}
    }
    (c_ty(l, ret, ret_ty), c)
}

/// Every reverse trampoline the module's `@c` imports need, with the callback signature shapes.
fn reverse_trampolines(l: &Lcx<'_>) -> IndexMap<Reverse, (Vec<CShape>, CShape)> {
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
                        let symbol = l.boxed_sym(&l.user_fn(f));
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

/// Declares the C function and the Dream-callable trampoline for a `@c` import.
pub(super) fn register_import(l: &mut Lcx<'_>, imp: &HImport) {
    let abi: Vec<Ty> = (0..imp.params.len())
        .flat_map(|i| c_param_tys(l, imp, i))
        .collect();
    let ret = c_ty(l, &imp.c_ret, imp.ret.unwrap_or_else(|| l.interner.int()));
    l.host(&import_host_name(imp), FnSig::plain(FnTy::new(ret, abi)));
    l.host("free", FnSig::plain(FnTy::new(Ty::Void, vec![Ty::Ptr])));
    let params = imp
        .params
        .iter()
        .enumerate()
        .map(|(i, t)| {
            if imp.param_by_ref.get(i).copied().unwrap_or(false) {
                l.h()
            } else {
                ll_ty(l.interner, *t, &l.h(), &l.word())
            }
        })
        .collect();
    let dream_ret = imp
        .ret
        .map(|t| ll_ty(l.interner, t, &l.h(), &l.word()))
        .unwrap_or(Ty::Void);
    register(l, &import_call_name(imp), dream_ret, params);
}

pub(super) fn register_reverse(l: &mut Lcx<'_>) {
    for (rev, (shapes, ret)) in reverse_trampolines(l) {
        let (r, ps) = reverse_sig(l, &rev, &shapes, &ret);
        register(l, &rev.symbol(), r, ps);
    }
}

pub(super) fn emit_reverse(l: &mut Lcx<'_>) {
    for (rev, (shapes, ret)) in reverse_trampolines(l) {
        reverse_trampoline(l, &rev, &shapes, &ret);
    }
}

// ---- forward: Dream -> C ------------------------------------------------------------------

pub(super) fn trampoline(l: &mut Lcx<'_>, imp: &HImport) {
    let real = import_host_name(imp);
    let wrap = import_call_name(imp);
    let mut fx = glue(l, &wrap);
    let mut args = Vec::new();
    let mut frees = Vec::new();
    for (i, ty) in imp.params.iter().enumerate() {
        let a = V {
            v: fx.w.param(i),
            unsigned: is_unsigned(fx.interner, *ty),
        };
        fx.c_arg(imp, i, *ty, &a, &mut args, &mut frees);
    }
    let r = fx.call(&real, &args);
    for f in frees {
        fx.call("free", &[f]);
    }
    let r = r.map(|r| fx.c_result(imp, &r));
    match r {
        Some(r) if !fx.w.ret.is_void() => {
            let t = fx.w.ret.clone();
            let r = fx.conv(&r, &t);
            fx.w.ret(Some(&r));
        }
        _ => fx.w.ret(None),
    }
    fx.finish();
}

impl<'l, 'a> Fx<'l, 'a> {
    fn is_zero(&mut self, a: &V) -> Value {
        self.w.icmp("eq", &a.v, &Value::zero(a.ty().clone()))
    }

    /// `a == 0 ? NULL : present(a)`.
    fn null_or(&mut self, a: &V, present: impl FnOnce(&mut Self) -> V) -> V {
        let z = self.is_zero(a);
        self.if_else_v(&z, |_| V::s(Value::null()), present)
    }

    /// The `raw` word of the `CPtr` stored at `at`, as `void*`.
    fn cptr_load(&mut self, at: &Value) -> V {
        let raw = self.load_ty(
            self.l.word(),
            at,
            self.l.cx.mir.layouts.target.ptr_align,
            true,
        );
        V::s(self.ptr(&raw))
    }

    fn c_arg(
        &mut self,
        imp: &HImport,
        i: usize,
        ty: TypeId,
        a: &V,
        args: &mut Vec<V>,
        frees: &mut Vec<V>,
    ) {
        match shape(imp, i).clone() {
            CShape::Void | CShape::Scalar => args.push(a.clone()),
            CShape::Ref => args.push(V::s(self.ptr(a))),
            CShape::Str { optional } => {
                let conv = if imp.c_wide_strings {
                    "dream_string_to_utf16z"
                } else {
                    "dream_string_to_utf8"
                };
                let s = if optional {
                    self.null_or(a, |fx| {
                        let s = fx.call_v(conv, std::slice::from_ref(a));
                        V::s(fx.ptr(&s))
                    })
                } else {
                    let s = self.call_v(conv, std::slice::from_ref(a));
                    V::s(self.ptr(&s))
                };
                frees.push(s.clone());
                args.push(s);
            }
            CShape::Ptr { optional: false } => {
                let at = self.ptr(a);
                let p = self.cptr_load(&at);
                args.push(p);
            }
            CShape::Ptr { optional: true } => {
                let (none, off) = self.option_layout(ty);
                let disc = {
                    let p = self.ptr(a);
                    self.load_ty(Ty::I32, &p, 4, false)
                };
                let is_none = self.w.icmp("eq", &disc.v, &Value::i32(none as i64));
                let p = self.if_else_v(
                    &is_none,
                    |_| V::s(Value::null()),
                    |fx| {
                        let at = fx.addr(a, off as i64);
                        fx.cptr_load(&at)
                    },
                );
                args.push(p);
            }
            s @ CShape::Func { optional, .. } => {
                let name = imp.name.clone();
                let fun_ty = callback_fun_ty(self.l, ty);
                let fp = if optional {
                    self.null_or(a, |fx| fx.fun_to_c(a, &s, fun_ty, &name))
                } else {
                    self.fun_to_c(a, &s, fun_ty, &name)
                };
                args.push(fp);
            }
            CShape::Callback {
                user_data_last,
                optional,
                ..
            } => {
                let rev = Reverse::Callback {
                    fun_ty: callback_fun_ty(self.l, ty),
                    user_data_last,
                };
                let tramp = V::s(self.l.fn_ref(&rev.symbol()));
                let (fp, ud) = if optional {
                    let fp = self.null_or(a, |_| tramp.clone());
                    (fp, V::s(self.ptr(a)))
                } else {
                    (tramp, V::s(self.ptr(a)))
                };
                args.push(fp);
                args.push(ud);
            }
            CShape::Array => {
                let data =
                    self.null_or(a, |fx| V::s(fx.addr(a, crate::abi::LEN_PREFIX_SIZE as i64)));
                args.push(data);
            }
        }
    }

    /// A C function pointer for the funcbox `a`: the Dream function itself when the signatures
    /// agree, else the per-target wrapper selected by its function-table index.
    fn fun_to_c(&mut self, a: &V, s: &CShape, fun_ty: TypeId, import: &str) -> V {
        let env = self.call_v("dream_funcbox_env", std::slice::from_ref(a));
        let captures = self.w.icmp("ne", &env.v, &Value::zero(env.ty().clone()));
        let msg = panic_msgs::c_capturing_closure(import);
        self.if_then(&captures, |fx| fx.panic_with(&msg));
        let idx = self.call_v("dream_funcbox_funcidx", std::slice::from_ref(a));
        if !s.needs_wrapper() {
            let f = self.call_v("dream_ft_get", &[idx]);
            return V::s(self.ptr(&f));
        }
        let mut fp = Value::null();
        for f in direct_targets(self.l, fun_ty) {
            let rev = Reverse::Direct {
                symbol: self.l.boxed_sym(&self.l.user_fn(f)),
                fun_ty,
            };
            let slot = self.l.cx.func_index(f) as i64;
            let hit = self.w.icmp("eq", &idx.v, &Value::i32(slot));
            let wrapper = self.l.fn_ref(&rev.symbol());
            fp = self.w.select(&hit, &wrapper, &fp);
        }
        let fp = V::s(fp);
        let missing = self.is_zero(&fp);
        let msg = panic_msgs::c_no_direct_target(import);
        self.if_then(&missing, |fx| fx.panic_with(&msg));
        fp
    }

    /// `(None discriminant, Some payload offset)` of an `Option<CPtr>`.
    fn option_layout(&self, ty: TypeId) -> (i32, u32) {
        let u = self
            .l
            .cx
            .nunion(ty)
            .unwrap_or_else(|| crate::internal_error!("Option<CPtr> without a union layout"));
        let none = u
            .variant("None")
            .unwrap_or_else(|| crate::internal_error!("Option without None"))
            .discriminant;
        let off = u
            .variant("Some")
            .and_then(|v| v.fields.first())
            .unwrap_or_else(|| crate::internal_error!("Option without a Some payload"))
            .offset;
        (none, off)
    }

    /// A fresh `Option<CPtr>` envelope holding `p` (`None` for `NULL`).
    fn option_cptr_new(&mut self, ty: TypeId, p: &V) -> V {
        let u = self
            .l
            .cx
            .nunion(ty)
            .unwrap_or_else(|| crate::internal_error!("Option<CPtr> without a union layout"));
        let size = u.size as i64;
        let some = u
            .variant("Some")
            .unwrap_or_else(|| crate::internal_error!("Option without Some"))
            .discriminant;
        let (none, off) = self.option_layout(ty);
        let tag = self.l.cx.type_tag(ty) as i64;
        let b = self.call_v("dream_malloc", &[V::i64(size), V::i32(tag)]);
        let bp = self.ptr(&b);
        self.memset0(&bp, &Value::i64(size));
        let z = self.is_zero(p);
        let disc = self
            .w
            .select(&z, &Value::i32(none as i64), &Value::i32(some as i64));
        self.store_ty(&Ty::I32, &bp, &V::s(disc), 4);
        let at = self.addr(&b, off as i64);
        let raw = V::u(self.conv(p, &self.l.word()));
        self.store_ty(
            &self.l.word(),
            &at,
            &raw,
            self.l.cx.mir.layouts.target.ptr_align,
        );
        self.as_ref(&b)
    }

    /// A fresh heap `CPtr` holding `p`, the value-struct result convention for native calls.
    fn cptr_new(&mut self, ty: TypeId, p: &V) -> V {
        let size = crate::backend::shared::abi_types::elem_size(&self.l.cx, ty) as i64;
        let tag = self.l.cx.type_tag(ty) as i64;
        let b = self.call_v("dream_malloc", &[V::i64(size), V::i32(tag)]);
        let bp = self.ptr(&b);
        let raw = V::u(self.conv(p, &self.l.word()));
        self.store_ty(
            &self.l.word(),
            &bp,
            &raw,
            self.l.cx.mir.layouts.target.ptr_align,
        );
        self.as_ref(&b)
    }

    /// A Dream `string` from a C string; `NULL` traps with `what` unless `optional`.
    fn c_string(&mut self, p: &V, optional: bool, what: &str) -> V {
        let z = self.is_zero(p);
        if optional {
            let h = self.h();
            return self.if_else_v(
                &z,
                |_| V::u(Value::zero(h)),
                |fx| fx.call_v("dream_utf8_to_string", std::slice::from_ref(p)),
            );
        }
        let msg = panic_msgs::c_null_string(what);
        self.if_then(&z, |fx| fx.panic_with(&msg));
        self.call_v("dream_utf8_to_string", std::slice::from_ref(p))
    }

    fn c_result(&mut self, imp: &HImport, r: &V) -> V {
        let ret_ty = imp.ret.unwrap_or_else(|| self.interner.int());
        match imp.c_ret.clone() {
            CShape::Str { optional } => {
                let what = panic_msgs::c_result_what(&imp.name);
                self.c_string(r, optional, &what)
            }
            CShape::Ptr { optional: false } => self.cptr_new(ret_ty, r),
            CShape::Ptr { optional: true } => self.option_cptr_new(ret_ty, r),
            _ => r.clone(),
        }
    }
}

// ---- reverse: C -> Dream ------------------------------------------------------------------

fn reverse_trampoline(l: &mut Lcx<'_>, rev: &Reverse, shapes: &[CShape], ret: &CShape) {
    let fun_ty = match rev {
        Reverse::Callback { fun_ty, .. } | Reverse::Direct { fun_ty, .. } => *fun_ty,
    };
    let (params, ret_ty) = fun_parts(l, fun_ty);
    let mut fx = glue(l, &rev.symbol());
    match rev {
        Reverse::Callback { user_data_last, .. } => {
            let object = fx.arg(if *user_data_last { shapes.len() } else { 0 });
            fx.call("dream_callback_check", &[object]);
        }
        Reverse::Direct { .. } => {
            fx.call("dream_callback_enter", &[]);
        }
    }
    let first = match rev {
        Reverse::Callback {
            user_data_last: false,
            ..
        } => 1,
        _ => 0,
    };
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
    let fp = match rev {
        Reverse::Callback { .. } => {
            let ud_i = if first == 1 { 0 } else { shapes.len() };
            let obj = fx.arg(ud_i);
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
        Reverse::Direct { symbol, .. } => fx.l.fn_ref(symbol),
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
        (Some(r), CShape::Scalar) => {
            let t = c_ty(fx.l, ret, ret_ty);
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
