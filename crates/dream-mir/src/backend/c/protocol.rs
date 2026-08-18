use super::ast::{CTy, CaseKey, Expr, Param, Stmt, SwitchArm, UnOp};
use super::builder::{FuncBuilder, ModuleBuilder};
use super::ctx::Cx;
use super::rvalue::{hash_code_of, to_string_fn};
use super::types::{c_ident, elem_size, load_cast};
use crate::backend::shared::func_symbol;
use dream_types::{TyKind, TypeId};

pub(super) fn emit_protocol(m: &mut ModuleBuilder, cx: &Cx<'_>) {
    let mut array_elems: Vec<_> = cx
        .mir
        .functions
        .iter()
        .flat_map(|f| f.locals.iter().map(|decl| decl.ty))
        .filter_map(|ty| match cx.interner.kind(ty) {
            TyKind::Array(elem) => Some(*elem),
            _ => None,
        })
        .collect();
    array_elems.extend(
        cx.native
            .structs
            .values()
            .flat_map(|layout| layout.fields.iter().map(|field| field.ty))
            .filter_map(|ty| match cx.interner.kind(ty) {
                TyKind::Array(elem) => Some(*elem),
                _ => None,
            }),
    );
    array_elems.sort_by_key(|ty| ty.0);
    array_elems.dedup();
    let user: std::collections::HashSet<String> =
        cx.mir.functions.iter().map(func_symbol).collect();
    let p = vec![Param {
        ty: CTy::Ptr,
        name: "p".into(),
    }];
    for elem in &array_elems {
        m.proto(
            CTy::Ptr,
            c_ident(&format!("array_to_string_t{}", elem.0)),
            p.clone(),
        );
    }
    for layout in cx.native.structs.values() {
        let sym = format!("{}_to_string", layout.name);
        if !user.contains(&sym) {
            m.proto(CTy::Ptr, c_ident(&sym), p.clone());
        }
        let hash = format!("{}_hash_code", layout.name);
        if !user.contains(&hash) {
            m.proto(CTy::I32, c_ident(&hash), p.clone());
        }
    }
    for layout in cx.native.unions.values() {
        let sym = format!("{}_to_string", layout.name);
        if !user.contains(&sym) {
            m.proto(CTy::Ptr, c_ident(&sym), p.clone());
        }
        let hash = format!("{}_hash_code", layout.name);
        if !user.contains(&hash) {
            m.proto(CTy::I32, c_ident(&hash), p.clone());
        }
    }
    for elem in array_elems {
        emit_array_to_string(m, cx, elem);
    }
    for (ty, layout) in &cx.native.structs {
        let sym = format!("{}_to_string", layout.name);
        if user.contains(&sym) {
            continue;
        }
        emit_struct_to_string(m, cx, *ty, &layout.name, &layout.fields);
    }
    for (ty, layout) in &cx.native.unions {
        let sym = format!("{}_to_string", layout.name);
        if user.contains(&sym) {
            continue;
        }
        emit_union_to_string(m, cx, *ty, &layout.name);
    }
    for (ty, layout) in &cx.native.structs {
        let hash = format!("{}_hash_code", layout.name);
        if !user.contains(&hash) {
            emit_struct_hash_code(m, cx, *ty, layout);
        }
    }
    for (ty, layout) in &cx.native.unions {
        let hash = format!("{}_hash_code", layout.name);
        if !user.contains(&hash) {
            emit_union_hash_code(m, cx, *ty, layout);
        }
    }
    emit_object_to_string_router(m, cx);
    emit_object_hash_code_router(m, cx);
}

fn concat_assign(r: &str, piece: Expr) -> Stmt {
    Stmt::block(vec![
        Stmt::decl(
            CTy::Ptr,
            "__c",
            Some(Expr::call("dream_concat_strings", vec![Expr::id(r), piece])),
        ),
        Stmt::call("dream_release", vec![Expr::id(r)]),
        Stmt::assign(Expr::id(r), Expr::id("__c")),
    ])
}

fn concat_conv(r: &str, conv: &str, value: Expr) -> Stmt {
    Stmt::block(vec![
        Stmt::decl(CTy::Ptr, "__p", Some(Expr::call(conv, vec![value]))),
        Stmt::decl(
            CTy::Ptr,
            "__c",
            Some(Expr::call(
                "dream_concat_strings",
                vec![Expr::id(r), Expr::id("__p")],
            )),
        ),
        Stmt::call("dream_release", vec![Expr::id(r)]),
        Stmt::call("dream_release", vec![Expr::id("__p")]),
        Stmt::assign(Expr::id(r), Expr::id("__c")),
    ])
}

fn emit_array_to_string(m: &mut ModuleBuilder, cx: &Cx<'_>, elem: TypeId) {
    let fn_name = c_ident(&format!("array_to_string_t{}", elem.0));
    let es = elem_size(cx, elem);
    let cast = load_cast(cx, elem);
    let conv = to_string_fn(cx, elem);
    let mut b = FuncBuilder::new(CTy::Ptr, fn_name);
    b.param(CTy::Ptr, "p");
    b.stmt(Stmt::decl(
        CTy::I32,
        "n",
        Some(Expr::ternary(
            Expr::id("p"),
            Expr::load(CTy::I32, Expr::dream_p(Expr::id("p"))),
            Expr::i(0),
        )),
    ));
    b.stmt(Stmt::decl(CTy::I32, "i", None));
    b.stmt(Stmt::decl(CTy::Ptr, "r", Some(Expr::id(cx.str_sym("[")))));
    let elem_ptr = Expr::add(
        Expr::ptr_add(Expr::id("p"), super::types::len_prefix()),
        Expr::mul(
            Expr::cast(CTy::Named("size_t"), Expr::id("i")),
            Expr::i(es as i64),
        ),
    );
    let mut loop_body = vec![Stmt::if_(
        Expr::id("i"),
        concat_assign("r", Expr::id(cx.str_sym(", "))),
    )];
    if cx.interner.is_value_type(elem) {
        loop_body.push(concat_conv("r", &conv, Expr::cast(CTy::Ptr, elem_ptr)));
    } else if conv.is_empty() {
        loop_body.push(concat_assign("r", Expr::load(cast, elem_ptr)));
    } else {
        loop_body.push(concat_conv("r", &conv, Expr::load(cast, elem_ptr)));
    }
    b.stmt(Stmt::For {
        init: Box::new(Stmt::assign(Expr::id("i"), Expr::i(0))),
        cond: Expr::lt(Expr::id("i"), Expr::id("n")),
        step: Box::new(Stmt::Expr(Expr::PostInc(Box::new(Expr::id("i"))))),
        body: Box::new(Stmt::block(loop_body)),
    });
    b.stmt(Stmt::block(vec![
        Stmt::decl(
            CTy::Ptr,
            "__c",
            Some(Expr::call(
                "dream_concat_strings",
                vec![Expr::id("r"), Expr::id(cx.str_sym("]"))],
            )),
        ),
        Stmt::call("dream_release", vec![Expr::id("r")]),
        Stmt::Return(Some(Expr::id("__c"))),
    ]));
    m.push_func(b);
}

fn emit_struct_to_string(
    m: &mut ModuleBuilder,
    cx: &Cx<'_>,
    ty: TypeId,
    name: &str,
    fields: &[dream_hir::FieldLayout],
) {
    let fn_name = c_ident(&format!("{name}_to_string"));
    let mut b = FuncBuilder::new(CTy::Ptr, fn_name);
    b.param(CTy::Ptr, "p");
    b.stmt(Stmt::if_(
        Expr::unary(UnOp::Not, Expr::id("p")),
        Stmt::Return(Some(Expr::id(cx.str_sym("null")))),
    ));
    let start = if matches!(cx.interner.kind(ty), TyKind::Tuple(_)) {
        "(".into()
    } else {
        format!("{name} {{ ")
    };
    b.stmt(Stmt::decl(
        CTy::Ptr,
        "r",
        Some(Expr::id(cx.str_sym(&start))),
    ));
    for (i, f) in fields.iter().enumerate() {
        let label = if matches!(cx.interner.kind(ty), TyKind::Tuple(_)) {
            if i == 0 {
                String::new()
            } else {
                ", ".into()
            }
        } else if i == 0 {
            format!("{}: ", f.name)
        } else {
            format!(", {}: ", f.name)
        };
        b.stmt(concat_assign("r", Expr::id(cx.str_sym(&label))));
        let value = field_value(cx, f);
        let text = to_string_fn(cx, f.ty);
        if text.is_empty() {
            b.stmt(concat_assign("r", value));
        } else {
            b.stmt(concat_conv("r", &text, value));
        }
    }
    let end = if matches!(cx.interner.kind(ty), TyKind::Tuple(_)) {
        ")"
    } else {
        " }"
    };
    b.stmt(Stmt::block(vec![
        Stmt::decl(
            CTy::Ptr,
            "__c",
            Some(Expr::call(
                "dream_concat_strings",
                vec![Expr::id("r"), Expr::id(cx.str_sym(end))],
            )),
        ),
        Stmt::call("dream_release", vec![Expr::id("r")]),
        Stmt::Return(Some(Expr::id("__c"))),
    ]));
    m.push_func(b);
}

fn emit_union_to_string(m: &mut ModuleBuilder, cx: &Cx<'_>, ty: TypeId, name: &str) {
    let fn_name = c_ident(&format!("{name}_to_string"));
    let mut b = FuncBuilder::new(CTy::Ptr, fn_name);
    b.param(CTy::Ptr, "p");
    let Some(layout) = cx.native.unions.get(&ty) else {
        b.stmt(Stmt::expr(Expr::cast(CTy::Void, Expr::id("p"))));
        b.ret(Some(Expr::id(cx.str_sym("<object>"))));
        m.push_func(b);
        return;
    };
    b.stmt(Stmt::decl(CTy::I32, "d", None));
    b.stmt(Stmt::decl(CTy::Ptr, "r", None));
    b.stmt(Stmt::if_(
        Expr::unary(UnOp::Not, Expr::id("p")),
        Stmt::Return(Some(Expr::id(cx.str_sym("null")))),
    ));
    b.assign(
        Expr::id("d"),
        Expr::load(CTy::I32, Expr::dream_p(Expr::id("p"))),
    );
    b.assign(Expr::id("r"), Expr::id(cx.str_sym("<object>")));
    let mut arms = Vec::new();
    for variant in &layout.variants {
        let (prefix, labels, suffix) = union_variant_pieces(variant);
        let mut body = vec![Stmt::assign(Expr::id("r"), Expr::id(cx.str_sym(&prefix)))];
        for (i, f) in variant.fields.iter().enumerate() {
            body.push(concat_assign("r", Expr::id(cx.str_sym(&labels[i]))));
            let value = field_value(cx, f);
            let text = to_string_fn(cx, f.ty);
            if text.is_empty() {
                body.push(concat_assign("r", value));
            } else {
                body.push(concat_conv("r", &text, value));
            }
        }
        if !suffix.is_empty() {
            body.push(concat_assign("r", Expr::id(cx.str_sym(&suffix))));
        }
        body.push(Stmt::Expr(Expr::id("break")));
        arms.push(SwitchArm {
            keys: vec![CaseKey::Int(variant.discriminant as i64)],
            body: vec![Stmt::block(body)],
        });
    }
    arms.push(SwitchArm {
        keys: vec![],
        body: vec![Stmt::Expr(Expr::id("break"))],
    });
    b.stmt(Stmt::Switch {
        expr: Expr::id("d"),
        arms,
    });
    b.ret(Some(Expr::id("r")));
    m.push_func(b);
}

fn union_variant_pieces(v: &dream_hir::UnionVariant) -> (String, Vec<String>, String) {
    if v.fields.is_empty() {
        return (v.name.clone(), Vec::new(), String::new());
    }
    let prefix = format!("{}(", v.name);
    let labels = v
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            if i == 0 {
                format!("{}: ", f.name)
            } else {
                format!(", {}: ", f.name)
            }
        })
        .collect();
    (prefix, labels, ")".to_string())
}

fn field_value(cx: &Cx<'_>, f: &dream_hir::FieldLayout) -> Expr {
    if cx.interner.is_value_type(f.ty) {
        Expr::cast(
            CTy::Ptr,
            Expr::ptr_add(Expr::id("p"), Expr::i(f.offset as i64)),
        )
    } else {
        Expr::load(
            load_cast(cx, f.ty),
            Expr::ptr_add(Expr::id("p"), Expr::i(f.offset as i64)),
        )
    }
}

fn emit_struct_hash_code(
    m: &mut ModuleBuilder,
    cx: &Cx<'_>,
    _ty: TypeId,
    layout: &dream_hir::TypeLayout,
) {
    let fn_name = c_ident(&format!("{}_hash_code", layout.name));
    let mut b = FuncBuilder::new(CTy::I32, fn_name);
    b.param(CTy::Ptr, "p");
    b.stmt(Stmt::decl(CTy::I32, "h", Some(Expr::i(17))));
    for f in &layout.fields {
        let hashed = hash_code_of(cx, f.ty, field_value(cx, f));
        b.assign(
            Expr::id("h"),
            Expr::add(Expr::mul(Expr::id("h"), Expr::i(31)), hashed),
        );
    }
    b.ret(Some(Expr::id("h")));
    m.push_func(b);
}

fn emit_union_hash_code(
    m: &mut ModuleBuilder,
    cx: &Cx<'_>,
    _ty: TypeId,
    layout: &dream_hir::UnionLayout,
) {
    let fn_name = c_ident(&format!("{}_hash_code", layout.name));
    let mut b = FuncBuilder::new(CTy::I32, fn_name);
    b.param(CTy::Ptr, "p");
    b.stmt(Stmt::decl(
        CTy::I32,
        "d",
        Some(Expr::ternary(
            Expr::id("p"),
            Expr::load(CTy::I32, Expr::dream_p(Expr::id("p"))),
            Expr::i(0),
        )),
    ));
    b.stmt(Stmt::decl(
        CTy::I32,
        "h",
        Some(Expr::add(
            Expr::mul(Expr::i(17), Expr::i(31)),
            Expr::id("d"),
        )),
    ));
    let mut arms = Vec::new();
    for variant in &layout.variants {
        let mut body = Vec::new();
        for f in &variant.fields {
            let hashed = hash_code_of(cx, f.ty, field_value(cx, f));
            body.push(Stmt::assign(
                Expr::id("h"),
                Expr::add(Expr::mul(Expr::id("h"), Expr::i(31)), hashed),
            ));
        }
        body.push(Stmt::Expr(Expr::id("break")));
        arms.push(SwitchArm {
            keys: vec![CaseKey::Int(variant.discriminant as i64)],
            body: vec![Stmt::block(body)],
        });
    }
    arms.push(SwitchArm {
        keys: vec![],
        body: vec![Stmt::Expr(Expr::id("break"))],
    });
    b.stmt(Stmt::Switch {
        expr: Expr::id("d"),
        arms,
    });
    b.ret(Some(Expr::id("h")));
    m.push_func(b);
}

fn emit_object_hash_code_router(m: &mut ModuleBuilder, cx: &Cx<'_>) {
    let mut b = FuncBuilder::new(CTy::I32, "dream_object_hash_code");
    b.param(CTy::Ptr, "p");
    b.stmt(Stmt::decl(CTy::I32, "tag", None));
    b.stmt(Stmt::if_(
        Expr::unary(UnOp::Not, Expr::id("p")),
        Stmt::Return(Some(Expr::i(0))),
    ));
    b.assign(
        Expr::id("tag"),
        Expr::call("dream_object_tag", vec![Expr::id("p")]),
    );
    let mut arms = vec![
        SwitchArm {
            keys: vec![
                CaseKey::Ident("TAG_INT"),
                CaseKey::Ident("TAG_UINT"),
                CaseKey::Ident("TAG_BOOL"),
                CaseKey::Ident("TAG_CHAR"),
                CaseKey::Ident("TAG_BYTE"),
            ],
            body: vec![Stmt::Return(Some(Expr::load(
                CTy::I32,
                Expr::dream_p(Expr::id("p")),
            )))],
        },
        SwitchArm {
            keys: vec![CaseKey::Ident("TAG_LONG"), CaseKey::Ident("TAG_ULONG")],
            body: vec![Stmt::Return(Some(Expr::call(
                "dream_hash_long",
                vec![Expr::load(CTy::I64, Expr::dream_p(Expr::id("p")))],
            )))],
        },
        SwitchArm {
            keys: vec![CaseKey::Ident("TAG_FLOAT")],
            body: vec![Stmt::Return(Some(Expr::call(
                "dream_bitcast_f32",
                vec![Expr::load(CTy::F32, Expr::dream_p(Expr::id("p")))],
            )))],
        },
        SwitchArm {
            keys: vec![CaseKey::Ident("TAG_DOUBLE")],
            body: vec![Stmt::Return(Some(Expr::call(
                "dream_hash_double",
                vec![Expr::load(CTy::F64, Expr::dream_p(Expr::id("p")))],
            )))],
        },
        SwitchArm {
            keys: vec![CaseKey::Ident("TAG_STRING")],
            body: vec![Stmt::Return(Some(Expr::call(
                "dream_string_hash",
                vec![Expr::id("p")],
            )))],
        },
    ];
    let mut tagged: Vec<_> = cx.tags.iter().collect();
    tagged.sort_by_key(|(_, t)| **t);
    for (ty, tag) in tagged {
        if let Some(l) = cx.native.structs.get(ty) {
            if matches!(cx.interner.kind(*ty), TyKind::Tuple(_)) {
                continue;
            }
            let fn_name = c_ident(&format!("{}_hash_code", l.name));
            arms.push(SwitchArm {
                keys: vec![CaseKey::Int(*tag as i64)],
                body: vec![Stmt::Return(Some(Expr::call(fn_name, vec![Expr::id("p")])))],
            });
        } else if let Some(l) = cx.native.unions.get(ty) {
            let fn_name = c_ident(&format!("{}_hash_code", l.name));
            arms.push(SwitchArm {
                keys: vec![CaseKey::Int(*tag as i64)],
                body: vec![Stmt::Return(Some(Expr::call(fn_name, vec![Expr::id("p")])))],
            });
        }
    }
    arms.push(SwitchArm {
        keys: vec![],
        body: vec![Stmt::Return(Some(Expr::cast(
            CTy::I32,
            Expr::cast(CTy::Named("uintptr_t"), Expr::id("p")),
        )))],
    });
    b.stmt(Stmt::Switch {
        expr: Expr::id("tag"),
        arms,
    });
    m.push_func(b);
}

fn emit_object_to_string_router(m: &mut ModuleBuilder, cx: &Cx<'_>) {
    let mut b = FuncBuilder::new(CTy::Ptr, "dream_object_to_string");
    b.param(CTy::Ptr, "p");
    b.stmt(Stmt::decl(CTy::I32, "tag", None));
    b.stmt(Stmt::if_(
        Expr::unary(UnOp::Not, Expr::id("p")),
        Stmt::Return(Some(Expr::id(cx.str_sym("null")))),
    ));
    b.assign(
        Expr::id("tag"),
        Expr::call("dream_object_tag", vec![Expr::id("p")]),
    );
    let load_i32 = Expr::load(CTy::I32, Expr::dream_p(Expr::id("p")));
    let mut arms = vec![
        arm_ret(
            "TAG_INT",
            Expr::call("dream_int_to_string", vec![load_i32.clone()]),
        ),
        arm_ret(
            "TAG_UINT",
            Expr::call("dream_uint_to_string", vec![load_i32.clone()]),
        ),
        arm_ret(
            "TAG_LONG",
            Expr::call(
                "dream_long_to_string",
                vec![Expr::load(CTy::I64, Expr::dream_p(Expr::id("p")))],
            ),
        ),
        arm_ret(
            "TAG_ULONG",
            Expr::call(
                "dream_ulong_to_string",
                vec![Expr::load(CTy::I64, Expr::dream_p(Expr::id("p")))],
            ),
        ),
        arm_ret(
            "TAG_BYTE",
            Expr::call("dream_byte_to_string", vec![load_i32.clone()]),
        ),
        arm_ret(
            "TAG_BOOL",
            Expr::call("dream_bool_to_string", vec![load_i32.clone()]),
        ),
        arm_ret(
            "TAG_CHAR",
            Expr::call("dream_char_to_string", vec![load_i32.clone()]),
        ),
        arm_ret(
            "TAG_FLOAT",
            Expr::call(
                "dream_float_to_string",
                vec![Expr::load(CTy::F32, Expr::dream_p(Expr::id("p")))],
            ),
        ),
        arm_ret(
            "TAG_DOUBLE",
            Expr::call(
                "dream_double_to_string",
                vec![Expr::load(CTy::F64, Expr::dream_p(Expr::id("p")))],
            ),
        ),
        SwitchArm {
            keys: vec![CaseKey::Ident("TAG_STRING")],
            body: vec![
                Stmt::call("dream_retain", vec![Expr::id("p")]),
                Stmt::Return(Some(Expr::id("p"))),
            ],
        },
        arm_ret(
            "TAG_ARRAY",
            Expr::call("dream_array_to_string", vec![Expr::id("p")]),
        ),
    ];
    let mut tagged: Vec<_> = cx.tags.iter().collect();
    tagged.sort_by_key(|(_, t)| **t);
    for (ty, tag) in tagged {
        if let Some(l) = cx.native.structs.get(ty) {
            if matches!(cx.interner.kind(*ty), TyKind::Tuple(_)) {
                continue;
            }
            let fn_name = c_ident(&format!("{}_to_string", l.name));
            arms.push(SwitchArm {
                keys: vec![CaseKey::Int(*tag as i64)],
                body: vec![Stmt::Return(Some(Expr::call(fn_name, vec![Expr::id("p")])))],
            });
        } else if let Some(l) = cx.native.unions.get(ty) {
            let fn_name = c_ident(&format!("{}_to_string", l.name));
            arms.push(SwitchArm {
                keys: vec![CaseKey::Int(*tag as i64)],
                body: vec![Stmt::Return(Some(Expr::call(fn_name, vec![Expr::id("p")])))],
            });
        }
    }
    arms.push(SwitchArm {
        keys: vec![],
        body: vec![Stmt::Return(Some(Expr::id(cx.str_sym("<object>"))))],
    });
    b.stmt(Stmt::Switch {
        expr: Expr::id("tag"),
        arms,
    });
    m.push_func(b);
    let mut print = FuncBuilder::new(CTy::Void, "dream_print_object");
    print.param(CTy::Ptr, "p");
    print.call(
        "print_string",
        vec![Expr::call("dream_object_to_string", vec![Expr::id("p")])],
    );
    m.push_func(print);
}

fn arm_ret(tag: &'static str, e: Expr) -> SwitchArm {
    SwitchArm {
        keys: vec![CaseKey::Ident(tag)],
        body: vec![Stmt::Return(Some(e))],
    }
}

pub(super) fn emit_iface_trampolines(m: &mut ModuleBuilder, cx: &Cx<'_>) {
    let max_tag = cx.tags.values().copied().max().unwrap_or(12);
    let ntags = (max_tag as usize) + 1;
    for (iid, iface) in cx.mir.interfaces.interfaces.iter().enumerate() {
        for slot in 0..iface.method_count {
            m.push(super::ast::Item::Global {
                thread_local: false,
                align: None,
                static_: true,
                const_: false,
                ty: CTy::Array {
                    elem: Box::new(CTy::VoidPtr),
                    len: ntags,
                },
                name: format!("dream_iface_{iid}_{slot}"),
                init: None,
            });
            let name = c_ident(&format!("__iface_dispatch_{iid}_{slot}"));
            let sig = iface.sigs[slot];
            let (td, ret, params) = super::types::fn_ptr_abi(cx.interner, sig);
            let mut b = FuncBuilder::new(ret, name);
            let mut pnames = Vec::new();
            for (i, pty) in params.iter().enumerate() {
                let n = if i == 0 {
                    "this".into()
                } else {
                    format!("a{}", i - 1)
                };
                b.param(pty.clone(), n.clone());
                pnames.push(n);
            }
            b.stmt(Stmt::decl(
                CTy::I32,
                "tag",
                Some(Expr::call("dream_object_tag", vec![Expr::id("this")])),
            ));
            b.stmt(Stmt::decl(
                CTy::Ident(td.clone()),
                "fn",
                Some(Expr::cast(
                    CTy::Ident(td),
                    Expr::index(
                        Expr::id(format!("dream_iface_{iid}_{slot}")),
                        Expr::id("tag"),
                    ),
                )),
            ));
            b.stmt(Stmt::if_(
                Expr::unary(UnOp::Not, Expr::id("fn")),
                Stmt::call("abort", vec![]),
            ));
            b.ret(Some(Expr::IndirectCall {
                callee: Box::new(Expr::id("fn")),
                args: pnames.into_iter().map(Expr::id).collect(),
            }));
            m.push_func(b);
        }
    }
}

pub(super) fn emit_iface_init(m: &mut ModuleBuilder, cx: &Cx<'_>) {
    let mut b = FuncBuilder::new(CTy::Void, "dream_init_itables");
    b.static_ = true;
    for imp in &cx.mir.interfaces.impls {
        let Some(tag) = interface_tag(cx, imp.class_ty) else {
            continue;
        };
        for (iid, symbols) in &imp.entries {
            for (slot, sym) in symbols.iter().enumerate() {
                let Some(f) = cx.mir.functions.iter().find(|f| f.name == *sym) else {
                    continue;
                };
                let cname = c_ident(&func_symbol(f));
                b.assign(
                    Expr::index(
                        Expr::id(format!("dream_iface_{iid}_{slot}")),
                        Expr::i(tag as i64),
                    ),
                    Expr::cast(CTy::VoidPtr, Expr::id(cname)),
                );
            }
        }
    }
    m.push_func(b);
}

fn interface_tag(cx: &Cx<'_>, ty: TypeId) -> Option<i32> {
    match cx.interner.kind(ty) {
        TyKind::Array(_) => Some(crate::abi::TAG_ARRAY),
        _ => cx.tags.get(&ty).copied(),
    }
}
