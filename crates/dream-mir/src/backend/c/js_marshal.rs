//! Struct/class and array `<-> js` marshalers for wasm32 C. Symbols and slot tags come from
//! [`dream_abi::js_abi`]; host calls use the same imported `js_*` bridges as the WAT path.

use super::ast::{CTy, Expr, Param, Stmt};
use super::builder::{FuncBuilder, ModuleBuilder};
use super::ctx::Cx;
use super::types::{c_ident, c_ty, elem_size, import_call_name, load_cast};
use crate::abi::LEN_PREFIX_SIZE;
use crate::backend::c::types::native_scalar_size;
use dream_abi::js_abi;
use dream_types::{method_fn, PrimTy, TyKind, TypeId};

pub(super) fn c_js_sym(wat: &str) -> String {
    c_ident(wat.trim_start_matches('$'))
}

pub(super) fn js_bridge(cx: &Cx<'_>, method: &str) -> String {
    let want = method_fn(js_abi::JS_TYPE, method);
    cx.mir
        .imports
        .iter()
        .find(|imp| imp.name == want)
        .map(|imp| import_call_name(cx.mir, imp))
        .unwrap_or_else(|| c_ident(&want))
}

pub(super) fn cast_c_sym(cx: &Cx<'_>, from: TypeId, to: TypeId) -> Option<String> {
    let is_js = |t: TypeId| matches!(cx.interner.kind(t), TyKind::Js);
    if is_js(to) {
        return cx
            .nstruct(from)
            .map(|l| c_js_sym(&js_abi::struct_to_js_sym(&l.name)));
    }
    if is_js(from) {
        return cx
            .nstruct(to)
            .map(|l| c_js_sym(&js_abi::js_to_struct_sym(&l.name)));
    }
    None
}

pub(super) fn emit_js_marshal(m: &mut ModuleBuilder, cx: &Cx<'_>) {
    if !cx.target.is_wasm32() || !crate::module_uses_js_bridges(cx.mir, cx.interner) {
        return;
    }
    let mut arrays = Vec::new();
    collect_array_elems(cx, &mut arrays);
    for (ty, layout) in &cx.native.structs {
        if matches!(cx.interner.kind(*ty), TyKind::Tuple(_)) {
            continue;
        }
        let to_js = c_js_sym(&js_abi::struct_to_js_sym(&layout.name));
        let from_js = c_js_sym(&js_abi::js_to_struct_sym(&layout.name));
        m.static_proto(
            CTy::Ptr,
            to_js,
            vec![Param {
                ty: CTy::Ptr,
                name: "this".into(),
            }],
        );
        if cx.interner.is_value_type(*ty) {
            m.static_proto(
                CTy::Void,
                from_js,
                vec![
                    Param {
                        ty: CTy::Ptr,
                        name: "j".into(),
                    },
                    Param {
                        ty: CTy::Ptr,
                        name: "dst".into(),
                    },
                ],
            );
        } else {
            m.static_proto(
                CTy::Ptr,
                from_js,
                vec![Param {
                    ty: CTy::Ptr,
                    name: "j".into(),
                }],
            );
        }
    }
    for (ty, layout) in &cx.native.unions {
        if !is_marshalable(cx, *ty) {
            continue;
        }
        let to_js = c_js_sym(&js_abi::struct_to_js_sym(&layout.name));
        let from_js = c_js_sym(&js_abi::js_to_struct_sym(&layout.name));
        m.static_proto(
            CTy::Ptr,
            to_js,
            vec![Param {
                ty: CTy::Ptr,
                name: "this".into(),
            }],
        );
        if cx.interner.is_value_union(*ty) {
            m.static_proto(
                CTy::Void,
                from_js,
                vec![
                    Param {
                        ty: CTy::Ptr,
                        name: "j".into(),
                    },
                    Param {
                        ty: CTy::Ptr,
                        name: "dst".into(),
                    },
                ],
            );
        } else {
            m.static_proto(
                CTy::Ptr,
                from_js,
                vec![Param {
                    ty: CTy::Ptr,
                    name: "j".into(),
                }],
            );
        }
    }
    for elem in &arrays {
        m.static_proto(
            CTy::Ptr,
            c_js_sym(&js_abi::array_to_js_sym(*elem)),
            vec![Param {
                ty: CTy::Ptr,
                name: "arr".into(),
            }],
        );
        m.static_proto(
            CTy::Ptr,
            c_js_sym(&js_abi::js_to_array_sym(*elem)),
            vec![Param {
                ty: CTy::Ptr,
                name: "j".into(),
            }],
        );
    }
    for (ty, layout) in &cx.native.unions {
        if !is_marshalable(cx, *ty) {
            continue;
        }
        emit_union_to_js(m, cx, *ty, layout);
        emit_js_to_union(m, cx, *ty, layout);
    }
    for (ty, layout) in &cx.native.structs {
        if matches!(cx.interner.kind(*ty), TyKind::Tuple(_)) {
            continue;
        }
        emit_struct_to_js(m, cx, layout);
        emit_js_to_struct(m, cx, *ty, layout);
    }
    for elem in &arrays {
        emit_array_to_js(m, cx, *elem);
        emit_js_to_array(m, cx, *elem);
    }
}

fn is_marshalable(cx: &Cx<'_>, ty: TypeId) -> bool {
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

fn is_option_union(layout: &dream_hir::UnionLayout) -> bool {
    let mut some = false;
    let mut none = false;
    if layout.variants.len() != 2 {
        return false;
    }
    for v in &layout.variants {
        match v.name.as_str() {
            "Some" if v.fields.len() == 1 => some = true,
            "None" if v.fields.is_empty() => none = true,
            _ => {}
        }
    }
    some && none
}

fn union_js_sym(layout: &dream_hir::UnionLayout) -> (String, String) {
    (
        c_js_sym(&js_abi::struct_to_js_sym(&layout.name)),
        c_js_sym(&js_abi::js_to_struct_sym(&layout.name)),
    )
}

fn collect_array_elems(cx: &Cx<'_>, out: &mut Vec<TypeId>) {
    let mut push = |ty: TypeId| {
        if let TyKind::Array(elem) = cx.interner.kind(ty) {
            if is_marshalable(cx, *elem) && !out.contains(elem) {
                out.push(*elem);
            }
        }
    };
    for layout in cx.native.structs.values() {
        for f in &layout.fields {
            push(f.ty);
        }
    }
    for f in &cx.mir.functions {
        for l in &f.locals {
            push(l.ty);
        }
    }
}

fn box_prim(p: PrimTy) -> (&'static str, bool) {
    match p {
        PrimTy::Int | PrimTy::UInt | PrimTy::Byte | PrimTy::Char => ("box_int", false),
        PrimTy::Long | PrimTy::ULong => ("box_long", false),
        PrimTy::Float | PrimTy::Double => ("box_double", matches!(p, PrimTy::Float)),
        PrimTy::Bool => ("box_bool", false),
        PrimTy::String => ("box_string", false),
    }
}

fn unbox_prim(p: PrimTy) -> (&'static str, bool) {
    match p {
        PrimTy::Int | PrimTy::UInt | PrimTy::Byte | PrimTy::Char => ("as_int", false),
        PrimTy::Long | PrimTy::ULong => ("as_long", false),
        PrimTy::Float => ("as_double", true),
        PrimTy::Double => ("as_double", false),
        PrimTy::Bool => ("as_bool", false),
        PrimTy::String => ("as_string", false),
    }
}

fn value_to_js(cx: &Cx<'_>, addr: Expr, ty: TypeId) -> Option<Expr> {
    match cx.interner.kind(ty) {
        TyKind::Prim(p) => {
            let (method, promote) = box_prim(*p);
            let mut v = Expr::load(load_cast(cx, ty), addr);
            if promote {
                v = Expr::cast(CTy::F64, v);
            }
            Some(Expr::call(js_bridge(cx, method), vec![v]))
        }
        TyKind::Enum(_) => Some(Expr::call(
            js_bridge(cx, "box_int"),
            vec![Expr::load(CTy::I32, addr)],
        )),
        TyKind::Js => Some(Expr::load(CTy::Ptr, addr)),
        TyKind::Array(elem) if is_marshalable(cx, *elem) => Some(Expr::call(
            c_js_sym(&js_abi::array_to_js_sym(*elem)),
            vec![Expr::load(CTy::Ptr, addr)],
        )),
        TyKind::Struct(..) if cx.interner.is_reference(ty) => {
            let name = cx.nstruct(ty)?.name.clone();
            Some(Expr::call(
                c_js_sym(&js_abi::struct_to_js_sym(&name)),
                vec![Expr::load(CTy::Ptr, addr)],
            ))
        }
        TyKind::Struct(..) => {
            let name = cx.nstruct(ty)?.name.clone();
            Some(Expr::call(
                c_js_sym(&js_abi::struct_to_js_sym(&name)),
                vec![addr],
            ))
        }
        TyKind::Union(..) if is_marshalable(cx, ty) => {
            let name = cx.nunion(ty)?.name.clone();
            let ptr = if cx.interner.is_value_union(ty) {
                addr
            } else {
                Expr::load(CTy::Ptr, addr)
            };
            Some(Expr::call(
                c_js_sym(&js_abi::struct_to_js_sym(&name)),
                vec![ptr],
            ))
        }
        _ => None,
    }
}

fn held_to_js(cx: &Cx<'_>, val: Expr, ty: TypeId) -> Option<Expr> {
    match cx.interner.kind(ty) {
        TyKind::Prim(p) => {
            let (method, promote) = box_prim(*p);
            let v = if promote {
                Expr::cast(CTy::F64, val)
            } else {
                val
            };
            Some(Expr::call(js_bridge(cx, method), vec![v]))
        }
        TyKind::Enum(_) => Some(Expr::call(js_bridge(cx, "box_int"), vec![val])),
        TyKind::Js => Some(val),
        TyKind::Array(elem) if is_marshalable(cx, *elem) => Some(Expr::call(
            c_js_sym(&js_abi::array_to_js_sym(*elem)),
            vec![val],
        )),
        TyKind::Struct(..) | TyKind::Union(..) => {
            let name = if matches!(cx.interner.kind(ty), TyKind::Union(..)) {
                cx.nunion(ty)?.name.clone()
            } else {
                cx.nstruct(ty)?.name.clone()
            };
            Some(Expr::call(
                c_js_sym(&js_abi::struct_to_js_sym(&name)),
                vec![val],
            ))
        }
        _ => None,
    }
}

fn value_from_js(cx: &Cx<'_>, jsval: Expr, ty: TypeId) -> Option<Expr> {
    match cx.interner.kind(ty) {
        TyKind::Prim(p) => {
            let (method, demote) = unbox_prim(*p);
            let v = Expr::call(js_bridge(cx, method), vec![jsval]);
            Some(if demote { Expr::cast(CTy::F32, v) } else { v })
        }
        TyKind::Enum(_) => Some(Expr::call(js_bridge(cx, "as_int"), vec![jsval])),
        TyKind::Js => Some(jsval),
        TyKind::Array(elem) if is_marshalable(cx, *elem) => Some(Expr::call(
            c_js_sym(&js_abi::js_to_array_sym(*elem)),
            vec![jsval],
        )),
        TyKind::Struct(..) if cx.interner.is_reference(ty) => {
            let name = cx.nstruct(ty)?.name.clone();
            Some(Expr::call(
                c_js_sym(&js_abi::js_to_struct_sym(&name)),
                vec![jsval],
            ))
        }
        TyKind::Union(..) if is_marshalable(cx, ty) && !cx.interner.is_value_union(ty) => {
            let name = cx.nunion(ty)?.name.clone();
            Some(Expr::call(
                c_js_sym(&js_abi::js_to_struct_sym(&name)),
                vec![jsval],
            ))
        }
        _ => None,
    }
}

fn write_from_js(cx: &Cx<'_>, dst: Expr, jsval: Expr, ty: TypeId) -> Option<Stmt> {
    if matches!(cx.interner.kind(ty), TyKind::Struct(..)) && cx.interner.is_value_type(ty) {
        let name = cx.nstruct(ty)?.name.clone();
        return Some(Stmt::call(
            c_js_sym(&js_abi::js_to_struct_sym(&name)),
            vec![jsval, dst],
        ));
    }
    if matches!(cx.interner.kind(ty), TyKind::Union(..)) && cx.interner.is_value_union(ty) {
        let name = cx.nunion(ty)?.name.clone();
        return Some(Stmt::call(
            c_js_sym(&js_abi::js_to_struct_sym(&name)),
            vec![jsval, dst],
        ));
    }
    let val = value_from_js(cx, jsval, ty)?;
    Some(Stmt::store(c_ty(cx.interner, ty), dst, val))
}

fn emit_struct_to_js(m: &mut ModuleBuilder, cx: &Cx<'_>, layout: &dream_hir::TypeLayout) {
    let mut b = FuncBuilder::new(CTy::Ptr, c_js_sym(&js_abi::struct_to_js_sym(&layout.name)));
    b.static_ = true;
    b.param(CTy::Ptr, "this");
    let o = b.temp(CTy::Ptr, Some(Expr::call(js_bridge(cx, "object"), vec![])));
    for f in &layout.fields {
        let addr = Expr::add(Expr::id("this"), Expr::i(f.offset as i64));
        let Some(val) = value_to_js(cx, addr, f.ty) else {
            continue;
        };
        b.call(
            js_bridge(cx, "set"),
            vec![o.clone(), Expr::id(cx.str_sym(&f.name).to_string()), val],
        );
    }
    b.ret(Some(o));
    m.push_func(b);
}

fn emit_js_to_struct(
    m: &mut ModuleBuilder,
    cx: &Cx<'_>,
    ty: TypeId,
    layout: &dream_hir::TypeLayout,
) {
    let is_value = cx.interner.is_value_type(ty);
    let mut b = if is_value {
        let mut b = FuncBuilder::new(CTy::Void, c_js_sym(&js_abi::js_to_struct_sym(&layout.name)));
        b.param(CTy::Ptr, "j");
        b.param(CTy::Ptr, "dst");
        b
    } else {
        let mut b = FuncBuilder::new(CTy::Ptr, c_js_sym(&js_abi::js_to_struct_sym(&layout.name)));
        b.param(CTy::Ptr, "j");
        b
    };
    b.static_ = true;
    let base = if is_value {
        Expr::id("dst")
    } else {
        let tag = cx.type_tag(ty, dream_types::DefId(0));
        b.temp(
            CTy::Ptr,
            Some(Expr::call(
                "dream_malloc",
                vec![Expr::i(layout.size as i64), Expr::i(tag as i64)],
            )),
        )
    };
    for f in &layout.fields {
        let dst = Expr::add(base.clone(), Expr::i(f.offset as i64));
        let jsval = Expr::call(
            js_bridge(cx, "get"),
            vec![Expr::id("j"), Expr::id(cx.str_sym(&f.name).to_string())],
        );
        if let Some(st) = write_from_js(cx, dst, jsval, f.ty) {
            b.stmt(st);
        } else {
            let (size, _) = native_scalar_size(cx, f.ty);
            b.call(
                "memset",
                vec![
                    Expr::ptr_add(base.clone(), Expr::i(f.offset as i64)),
                    Expr::i(0),
                    Expr::i(size as i64),
                ],
            );
        }
    }
    if !is_value {
        b.ret(Some(base));
    }
    m.push_func(b);
}

fn js_null(cx: &Cx<'_>) -> Expr {
    Expr::call(js_bridge(cx, "host_null"), vec![])
}

fn js_is_null(cx: &Cx<'_>, j: Expr) -> Expr {
    Expr::call(js_bridge(cx, "host_is_null"), vec![j])
}

fn emit_union_to_js(
    m: &mut ModuleBuilder,
    cx: &Cx<'_>,
    ty: TypeId,
    layout: &dream_hir::UnionLayout,
) {
    let (to_js, _) = union_js_sym(layout);
    let mut b = FuncBuilder::new(CTy::Ptr, to_js);
    b.static_ = true;
    b.param(CTy::Ptr, "this");
    let this = Expr::id("this");
    if cx.interner.is_niche_union(ty) {
        let payload_ty = layout
            .variants
            .iter()
            .find(|v| !v.fields.is_empty())
            .and_then(|v| v.fields.first())
            .map(|f| f.ty);
        let Some(payload_ty) = payload_ty else {
            crate::internal_error!("niche union missing payload field");
        };
        let some = held_to_js(cx, this.clone(), payload_ty)
            .unwrap_or_else(|| crate::internal_error!("niche union payload should be marshalable"));
        b.ret(Some(Expr::ternary(
            Expr::eq(this, Expr::Null),
            js_null(cx),
            some,
        )));
        m.push_func(b);
        return;
    }
    if is_option_union(layout) {
        let none = layout
            .variants
            .iter()
            .find(|v| v.name == "None")
            .unwrap_or_else(|| crate::internal_error!("Option union missing None"));
        let some = layout
            .variants
            .iter()
            .find(|v| v.name == "Some")
            .unwrap_or_else(|| crate::internal_error!("Option union missing Some"));
        let payload = &some.fields[0];
        let disc = Expr::load(CTy::I32, Expr::dream_p(this.clone()));
        let inner = value_to_js(
            cx,
            Expr::ptr_add(this, Expr::i(payload.offset as i64)),
            payload.ty,
        )
        .unwrap_or_else(|| crate::internal_error!("Option payload should be marshalable"));
        b.ret(Some(Expr::ternary(
            Expr::eq(disc, Expr::i(none.discriminant as i64)),
            js_null(cx),
            inner,
        )));
        m.push_func(b);
        return;
    }
    let o = b.temp(CTy::Ptr, Some(Expr::call(js_bridge(cx, "object"), vec![])));
    let disc = b.temp(
        CTy::I32,
        Some(Expr::load(CTy::I32, Expr::dream_p(this.clone()))),
    );
    for v in &layout.variants {
        let mut body = Vec::new();
        body.push(Stmt::call(
            js_bridge(cx, "set"),
            vec![
                o.clone(),
                Expr::id(cx.str_sym("type").to_string()),
                Expr::call(
                    js_bridge(cx, "box_string"),
                    vec![Expr::id(cx.str_sym(&v.name).to_string())],
                ),
            ],
        ));
        for f in &v.fields {
            let addr = Expr::ptr_add(this.clone(), Expr::i(f.offset as i64));
            let Some(val) = value_to_js(cx, addr, f.ty) else {
                continue;
            };
            body.push(Stmt::call(
                js_bridge(cx, "set"),
                vec![o.clone(), Expr::id(cx.str_sym(&f.name).to_string()), val],
            ));
        }
        body.push(Stmt::Return(Some(o.clone())));
        b.stmt(Stmt::if_(
            Expr::eq(disc.clone(), Expr::i(v.discriminant as i64)),
            Stmt::Block(body),
        ));
    }
    b.ret(Some(o));
    m.push_func(b);
}

fn alloc_union(cx: &Cx<'_>, ty: TypeId, layout: &dream_hir::UnionLayout) -> Expr {
    let tag = cx.type_tag(ty, dream_types::DefId(0));
    Expr::call(
        "dream_malloc",
        vec![Expr::i(layout.size as i64), Expr::i(tag as i64)],
    )
}

fn write_union_fields(
    cx: &Cx<'_>,
    base: &Expr,
    variant: &dream_hir::UnionVariant,
    field_js: &[Expr],
) -> Vec<Stmt> {
    let mut stmts = Vec::new();
    for (f, jsval) in variant.fields.iter().zip(field_js.iter()) {
        let dst = Expr::ptr_add(base.clone(), Expr::i(f.offset as i64));
        if let Some(st) = write_from_js(cx, dst, jsval.clone(), f.ty) {
            stmts.push(st);
        }
    }
    stmts
}

fn emit_js_to_union(
    m: &mut ModuleBuilder,
    cx: &Cx<'_>,
    ty: TypeId,
    layout: &dream_hir::UnionLayout,
) {
    let (_, from_js) = union_js_sym(layout);
    let is_value = cx.interner.is_value_union(ty);
    let mut b = if is_value {
        let mut b = FuncBuilder::new(CTy::Void, from_js);
        b.param(CTy::Ptr, "j");
        b.param(CTy::Ptr, "dst");
        b
    } else {
        let mut b = FuncBuilder::new(CTy::Ptr, from_js);
        b.param(CTy::Ptr, "j");
        b
    };
    b.static_ = true;
    let j = Expr::id("j");
    if cx.interner.is_niche_union(ty) {
        let payload_ty = layout
            .variants
            .iter()
            .find(|v| !v.fields.is_empty())
            .and_then(|v| v.fields.first())
            .map(|f| f.ty)
            .unwrap_or_else(|| crate::internal_error!("niche union missing payload"));
        let some = value_from_js(cx, j.clone(), payload_ty)
            .unwrap_or_else(|| crate::internal_error!("niche union payload should be marshalable"));
        b.ret(Some(Expr::ternary(js_is_null(cx, j), Expr::Null, some)));
        m.push_func(b);
        return;
    }
    let base = if is_value {
        Expr::id("dst")
    } else {
        b.temp(CTy::Ptr, Some(alloc_union(cx, ty, layout)))
    };
    b.call(
        "memset",
        vec![
            Expr::dream_p(base.clone()),
            Expr::i(0),
            Expr::i(layout.size as i64),
        ],
    );
    if is_option_union(layout) {
        let none = layout
            .variants
            .iter()
            .find(|v| v.name == "None")
            .unwrap_or_else(|| crate::internal_error!("Option union missing None"));
        let some = layout
            .variants
            .iter()
            .find(|v| v.name == "Some")
            .unwrap_or_else(|| crate::internal_error!("Option union missing Some"));
        let payload = &some.fields[0];
        let dst = Expr::ptr_add(base.clone(), Expr::i(payload.offset as i64));
        let write_some = write_from_js(cx, dst, j.clone(), payload.ty)
            .unwrap_or_else(|| crate::internal_error!("Option payload should be marshalable"));
        let then_s = Stmt::Block(vec![
            Stmt::store(
                CTy::I32,
                Expr::dream_p(base.clone()),
                Expr::i(some.discriminant as i64),
            ),
            write_some,
        ]);
        let else_s = Stmt::store(
            CTy::I32,
            Expr::dream_p(base.clone()),
            Expr::i(none.discriminant as i64),
        );
        b.stmt(Stmt::if_else(js_is_null(cx, j), else_s, then_s));
        if !is_value {
            b.ret(Some(base));
        }
        m.push_func(b);
        return;
    }
    let tag = b.temp(
        CTy::Ptr,
        Some(Expr::call(
            js_bridge(cx, "as_string"),
            vec![Expr::call(
                js_bridge(cx, "get"),
                vec![j.clone(), Expr::id(cx.str_sym("type").to_string())],
            )],
        )),
    );
    for v in &layout.variants {
        let cond = Expr::call(
            "dream_string_eq",
            vec![tag.clone(), Expr::id(cx.str_sym(&v.name).to_string())],
        );
        let field_js: Vec<Expr> = v
            .fields
            .iter()
            .map(|f| {
                Expr::call(
                    js_bridge(cx, "get"),
                    vec![j.clone(), Expr::id(cx.str_sym(&f.name).to_string())],
                )
            })
            .collect();
        let mut body = vec![Stmt::store(
            CTy::I32,
            Expr::dream_p(base.clone()),
            Expr::i(v.discriminant as i64),
        )];
        body.extend(write_union_fields(cx, &base, v, &field_js));
        if !is_value {
            body.push(Stmt::Return(Some(base.clone())));
        }
        b.stmt(Stmt::if_(cond, Stmt::Block(body)));
    }
    if !is_value {
        b.ret(Some(base));
    }
    m.push_func(b);
}

fn emit_array_to_js(m: &mut ModuleBuilder, cx: &Cx<'_>, elem: TypeId) {
    let esize = elem_size(cx, elem);
    let mut b = FuncBuilder::new(CTy::Ptr, c_js_sym(&js_abi::array_to_js_sym(elem)));
    b.static_ = true;
    b.param(CTy::Ptr, "arr");
    let o = b.temp(CTy::Ptr, Some(Expr::call(js_bridge(cx, "array"), vec![])));
    b.stmt(Stmt::decl(
        CTy::I32,
        "n",
        Some(Expr::ternary(
            Expr::id("arr"),
            Expr::load(CTy::I32, Expr::id("arr")),
            Expr::i(0),
        )),
    ));
    b.stmt(Stmt::decl(CTy::I32, "i", Some(Expr::i(0))));
    let addr = Expr::add(
        Expr::add(Expr::id("arr"), Expr::i(LEN_PREFIX_SIZE as i64)),
        Expr::mul(Expr::id("i"), Expr::i(esize as i64)),
    );
    let val = value_to_js(cx, addr, elem).expect("array element is marshalable");
    b.stmt(Stmt::For {
        init: Box::new(Stmt::assign(Expr::id("i"), Expr::i(0))),
        cond: Expr::lt(Expr::id("i"), Expr::id("n")),
        step: Box::new(Stmt::Expr(Expr::PostInc(Box::new(Expr::id("i"))))),
        body: Box::new(Stmt::call(
            js_bridge(cx, "index_set"),
            vec![
                o.clone(),
                Expr::call(js_bridge(cx, "box_int"), vec![Expr::id("i")]),
                val,
            ],
        )),
    });
    b.ret(Some(o));
    m.push_func(b);
}

fn emit_js_to_array(m: &mut ModuleBuilder, cx: &Cx<'_>, elem: TypeId) {
    let esize = elem_size(cx, elem);
    let mut b = FuncBuilder::new(CTy::Ptr, c_js_sym(&js_abi::js_to_array_sym(elem)));
    b.static_ = true;
    b.param(CTy::Ptr, "j");
    let n = b.temp(
        CTy::I32,
        Some(Expr::call(
            js_bridge(cx, "as_int"),
            vec![Expr::call(
                js_bridge(cx, "get"),
                vec![Expr::id("j"), Expr::id(cx.str_sym("length").to_string())],
            )],
        )),
    );
    let o = b.temp(
        CTy::Ptr,
        Some(Expr::call(
            "dream_array_new",
            vec![n.clone(), Expr::i(esize as i64)],
        )),
    );
    b.stmt(Stmt::decl(CTy::I32, "i", Some(Expr::i(0))));
    let dst = Expr::add(
        Expr::add(o.clone(), Expr::i(LEN_PREFIX_SIZE as i64)),
        Expr::mul(Expr::id("i"), Expr::i(esize as i64)),
    );
    let jsval = Expr::call(
        js_bridge(cx, "index_get"),
        vec![
            Expr::id("j"),
            Expr::call(js_bridge(cx, "box_int"), vec![Expr::id("i")]),
        ],
    );
    let body = write_from_js(cx, dst, jsval, elem)
        .unwrap_or_else(|| crate::internal_error!("array element should be marshalable"));
    b.stmt(Stmt::For {
        init: Box::new(Stmt::assign(Expr::id("i"), Expr::i(0))),
        cond: Expr::lt(Expr::id("i"), n),
        step: Box::new(Stmt::Expr(Expr::PostInc(Box::new(Expr::id("i"))))),
        body: Box::new(body),
    });
    b.ret(Some(o));
    m.push_func(b);
}
