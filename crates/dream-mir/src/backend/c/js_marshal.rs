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
        return cx.nstruct(from).map(|l| c_js_sym(&js_abi::struct_to_js_sym(&l.name)));
    }
    if is_js(from) {
        return cx.nstruct(to).map(|l| c_js_sym(&js_abi::js_to_struct_sym(&l.name)));
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
    match cx.interner.kind(ty) {
        TyKind::Prim(_) | TyKind::Enum(_) | TyKind::Js => true,
        TyKind::Array(elem) => is_marshalable(cx, *elem),
        TyKind::Struct(..) => true,
        _ => false,
    }
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
        _ => None,
    }
}

fn value_from_js(cx: &Cx<'_>, jsval: Expr, ty: TypeId) -> Option<Expr> {
    match cx.interner.kind(ty) {
        TyKind::Prim(p) => {
            let (method, demote) = unbox_prim(*p);
            let v = Expr::call(js_bridge(cx, method), vec![jsval]);
            Some(if demote {
                Expr::cast(CTy::F32, v)
            } else {
                v
            })
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
    let val = value_from_js(cx, jsval, ty)?;
    Some(Stmt::store(c_ty(cx.interner, ty), dst, val))
}

fn emit_struct_to_js(m: &mut ModuleBuilder, cx: &Cx<'_>, layout: &dream_hir::TypeLayout) {
    let mut b = FuncBuilder::new(CTy::Ptr, c_js_sym(&js_abi::struct_to_js_sym(&layout.name)));
    b.static_ = true;
    b.param(CTy::Ptr, "this");
    let o = b.temp(
        CTy::Ptr,
        Some(Expr::call(js_bridge(cx, "object"), vec![])),
    );
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
            vec![
                Expr::id("j"),
                Expr::id(cx.str_sym(&f.name).to_string()),
            ],
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
