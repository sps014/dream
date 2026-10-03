//! Backend-neutral policy for wasm32 `<-> js` marshaling: which types marshal, the generated
//! marshaler symbols, and the `js.*` host bridges they call. Symbols and slot tags come from
//! [`dream_abi::js_abi`].

use super::abi_types::{c_ident, import_call_name};
use super::cx::Cx;
use dream_abi::js_abi;
use dream_types::{method_fn, PrimTy, TyKind, TypeId};

/// Whether this module gets marshalers at all (wasm32 modules that call a `js.*` bridge).
pub(crate) fn emits_js_marshal(cx: &Cx<'_>) -> bool {
    cx.target.is_wasm32() && crate::module_uses_js_bridges(cx.mir, cx.interner)
}

pub(crate) fn js_sym(wat: &str) -> String {
    c_ident(wat.trim_start_matches('$'))
}

/// The callable symbol of the `js.<method>` host bridge.
pub(crate) fn js_bridge(cx: &Cx<'_>, method: &str) -> String {
    let want = method_fn(js_abi::JS_TYPE, method);
    cx.mir
        .imports
        .iter()
        .find(|imp| imp.name == want)
        .map(import_call_name)
        .unwrap_or_else(|| c_ident(&want))
}

/// The generated marshaler a `from as to` cast between a struct and `js` calls.
pub(crate) fn cast_sym(cx: &Cx<'_>, from: TypeId, to: TypeId) -> Option<String> {
    let is_js = |t: TypeId| matches!(cx.interner.kind(t), TyKind::Js);
    if is_js(to) {
        return cx
            .nstruct(from)
            .map(|l| js_sym(&js_abi::struct_to_js_sym(&l.name)));
    }
    if is_js(from) {
        return cx
            .nstruct(to)
            .map(|l| js_sym(&js_abi::js_to_struct_sym(&l.name)));
    }
    None
}

/// `(to_js, from_js)` symbols of a struct or union named `name`.
pub(crate) fn struct_syms(name: &str) -> (String, String) {
    (
        js_sym(&js_abi::struct_to_js_sym(name)),
        js_sym(&js_abi::js_to_struct_sym(name)),
    )
}

/// `(to_js, from_js)` symbols of an array of `elem`.
pub(crate) fn array_syms(elem: TypeId) -> (String, String) {
    (
        js_sym(&js_abi::array_to_js_sym(elem)),
        js_sym(&js_abi::js_to_array_sym(elem)),
    )
}

pub(crate) fn is_marshalable(cx: &Cx<'_>, ty: TypeId) -> bool {
    is_marshalable_rec(cx, ty, &mut Vec::new())
}

fn is_marshalable_rec(cx: &Cx<'_>, ty: TypeId, stack: &mut Vec<TypeId>) -> bool {
    if stack.contains(&ty) {
        return true;
    }
    stack.push(ty);
    let ok = match cx.interner.kind(ty) {
        TyKind::Prim(_) | TyKind::Enum(_) | TyKind::Js => true,
        TyKind::Array(elem) => is_marshalable_rec(cx, *elem, stack),
        TyKind::Struct(..) => true,
        TyKind::Union(..) => cx.nunion(ty).is_some_and(|u| {
            u.variants
                .iter()
                .all(|v| v.fields.iter().all(|f| is_marshalable_rec(cx, f.ty, stack)))
        }),
        _ => false,
    };
    stack.pop();
    ok
}

/// `Some(payload)` / `None` shaped unions marshal as the payload or JS `null`.
pub(crate) fn is_option_union(layout: &dream_hir::UnionLayout) -> bool {
    if layout.variants.len() != 2 {
        return false;
    }
    let some = layout
        .variants
        .iter()
        .any(|v| v.name == "Some" && v.fields.len() == 1);
    let none = layout
        .variants
        .iter()
        .any(|v| v.name == "None" && v.fields.is_empty());
    some && none
}

/// Element types of every marshalable array a struct field or local holds, in first-seen order.
pub(crate) fn array_elems(cx: &Cx<'_>) -> Vec<TypeId> {
    let mut out: Vec<TypeId> = Vec::new();
    let mut push = |ty: TypeId| {
        if let TyKind::Array(elem) = cx.interner.kind(ty) {
            if is_marshalable(cx, *elem) && !out.contains(elem) {
                out.push(*elem);
            }
        }
    };
    for layout in cx.mir.layouts.structs.values() {
        for f in &layout.fields {
            push(f.ty);
        }
    }
    for f in &cx.mir.functions {
        for l in &f.locals {
            push(l.ty);
        }
    }
    out
}

/// The `js.box_*` bridge for a primitive, and whether a `float` widens to `double` first.
pub(crate) fn box_prim(p: PrimTy) -> (&'static str, bool) {
    match p {
        PrimTy::Int | PrimTy::UInt | PrimTy::Byte | PrimTy::Char => ("box_int", false),
        PrimTy::Long | PrimTy::ULong | PrimTy::ISize | PrimTy::USize => ("box_long", false),
        PrimTy::Float | PrimTy::Double => ("box_double", matches!(p, PrimTy::Float)),
        PrimTy::Bool => ("box_bool", false),
        PrimTy::String => ("box_string", false),
    }
}

/// The `js.as_*` bridge for a primitive, and whether the `double` result narrows to `float`.
pub(crate) fn unbox_prim(p: PrimTy) -> (&'static str, bool) {
    match p {
        PrimTy::Int | PrimTy::UInt | PrimTy::Byte | PrimTy::Char => ("as_int", false),
        PrimTy::Long | PrimTy::ULong | PrimTy::ISize | PrimTy::USize => ("as_long", false),
        PrimTy::Float => ("as_double", true),
        PrimTy::Double => ("as_double", false),
        PrimTy::Bool => ("as_bool", false),
        PrimTy::String => ("as_string", false),
    }
}
