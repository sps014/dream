use super::ast::{CTy, CaseKey, Expr, Param, Stmt, SwitchArm, UnOp};
use super::builder::{FuncBuilder, ModuleBuilder};
use super::ctx::Cx;
use super::statements::value_ref_stmts;
use super::types::{c_ident, elem_size};
use crate::backend::shared::func_symbol;
use crate::Mir;
use dream_types::{TyKind, TypeId, TypeInterner};

pub(super) fn release_sym(interner: &TypeInterner, mir: &Mir, ty: TypeId) -> String {
    match interner.kind(ty) {
        TyKind::Struct(..) | TyKind::Union(..) => {
            if let Some(l) = mir.layouts.structs.get(&ty) {
                c_ident(&format!("release_{}", l.name))
            } else if let Some(l) = mir.layouts.unions.get(&ty) {
                c_ident(&format!("release_{}", l.name))
            } else {
                "dream_release".into()
            }
        }
        TyKind::Array(e) if interner.is_reference(*e) || interner.is_value_type(*e) => {
            c_ident(&format!("release_array_t{}", e.0))
        }
        TyKind::Func(..) => "dream_release_funcbox".into(),
        _ => "dream_release".into(),
    }
}

pub(super) fn destroy_sym(interner: &TypeInterner, mir: &Mir, ty: TypeId) -> String {
    if matches!(interner.kind(ty), TyKind::Js | TyKind::Func(..))
        || interner.is_shared_type(ty)
        || matches!(interner.kind(ty), TyKind::Prim(dream_types::PrimTy::String))
    {
        return release_sym(interner, mir, ty);
    }
    match interner.kind(ty) {
        TyKind::Struct(..) | TyKind::Union(..) => {
            if let Some(l) = mir.layouts.structs.get(&ty) {
                c_ident(&format!("destroy_{}", l.name))
            } else if let Some(l) = mir.layouts.unions.get(&ty) {
                c_ident(&format!("destroy_{}", l.name))
            } else {
                c_ident("destroy_object")
            }
        }
        TyKind::Array(e) if interner.is_reference(*e) || interner.is_value_type(*e) => {
            c_ident(&format!("destroy_array_t{}", e.0))
        }
        TyKind::Object | TyKind::Interface(..) => c_ident("destroy_object"),
        _ => "dream_destroy".into(),
    }
}

fn collect_array_elems(
    interner: &TypeInterner,
    f: &crate::MirFunction,
    array_elems: &mut std::collections::BTreeSet<TypeId>,
) {
    for local in &f.locals {
        if let TyKind::Array(e) = interner.kind(local.ty) {
            if interner.is_reference(*e) || interner.is_value_type(*e) {
                array_elems.insert(*e);
            }
        }
    }
}

fn rc_header() -> Stmt {
    Stmt::block(vec![
        Stmt::decl(
            CTy::ptr_to(CTy::I32),
            "rc",
            Some(Expr::cast(
                CTy::ptr_to(CTy::I32),
                Expr::add(Expr::char_p(Expr::id("p")), super::types::rc_delta()),
            )),
        ),
        Stmt::decl(CTy::I32, "old", Some(Expr::deref(Expr::id("rc")))),
        Stmt::if_(
            Expr::bin(
                crate::BinOp::Or,
                Expr::bin(crate::BinOp::Le, Expr::id("old"), Expr::i(0)),
                Expr::eq(Expr::id("old"), Expr::id("INT32_MAX")),
            ),
            Stmt::Return(None),
        ),
        Stmt::assign(
            Expr::deref(Expr::id("rc")),
            Expr::bin(crate::BinOp::Sub, Expr::id("old"), Expr::i(1)),
        ),
        Stmt::if_(Expr::ne(Expr::id("old"), Expr::i(1)), Stmt::Return(None)),
    ])
}

fn unique_header() -> Stmt {
    Stmt::block(vec![
        Stmt::decl(
            CTy::ptr_to(CTy::I32),
            "rc",
            Some(Expr::cast(
                CTy::ptr_to(CTy::I32),
                Expr::add(Expr::char_p(Expr::id("p")), super::types::rc_delta()),
            )),
        ),
        Stmt::decl(CTy::I32, "old", Some(Expr::deref(Expr::id("rc")))),
        Stmt::if_(
            Expr::eq(Expr::id("old"), Expr::id("INT32_MAX")),
            Stmt::Return(None),
        ),
    ])
}

fn maybe_defer(b: &mut FuncBuilder, destroy: &str, uses_defer: bool) {
    if !uses_defer {
        return;
    }
    let enq = Expr::call(
        "dream_defer_try_enqueue",
        vec![Expr::id("p"), Expr::id(destroy)],
    );
    b.stmt(Stmt::if_(
        Expr::and(Expr::unary(UnOp::Not, Expr::id("dream_defer_busy")), enq),
        Stmt::Return(None),
    ));
}

pub(super) fn emit_release_helpers(m: &mut ModuleBuilder, cx: &Cx<'_>) {
    let mir = cx.mir;
    let interner = cx.interner;
    let mut array_elems = std::collections::BTreeSet::new();
    for layout in cx.native.structs.values() {
        for f in &layout.fields {
            if let TyKind::Array(e) = interner.kind(f.ty) {
                if interner.is_reference(*e) || interner.is_value_type(*e) {
                    array_elems.insert(*e);
                }
            }
        }
    }
    for f in &mir.functions {
        collect_array_elems(interner, f, &mut array_elems);
        if f.is_async {
            if let Some(hir) = f.hir_fn.as_ref() {
                let body = crate::lower::lower_async_poll_body(hir, interner);
                collect_array_elems(interner, &body, &mut array_elems);
            }
        }
    }
    let p = vec![Param {
        ty: CTy::Ptr,
        name: "p".into(),
    }];
    for elem in &array_elems {
        m.static_proto(
            CTy::Void,
            c_ident(&format!("release_array_t{}", elem.0)),
            p.clone(),
        );
    }
    for layout in cx.native.structs.values() {
        m.static_proto(
            CTy::Void,
            c_ident(&format!("release_{}", layout.name)),
            p.clone(),
        );
    }
    for layout in cx.native.unions.values() {
        m.static_proto(
            CTy::Void,
            c_ident(&format!("release_{}", layout.name)),
            p.clone(),
        );
    }
    if mir.uses_defer {
        for elem in &array_elems {
            m.static_proto(
                CTy::Void,
                c_ident(&format!("destroy_array_t{}", elem.0)),
                p.clone(),
            );
        }
        for layout in cx.native.structs.values() {
            m.static_proto(
                CTy::Void,
                c_ident(&format!("destroy_{}", layout.name)),
                p.clone(),
            );
        }
        for layout in cx.native.unions.values() {
            m.static_proto(
                CTy::Void,
                c_ident(&format!("destroy_{}", layout.name)),
                p.clone(),
            );
        }
        m.static_proto(CTy::Void, c_ident("destroy_object"), p.clone());
    }
    let array_elems_copy = array_elems.clone();
    for elem in array_elems {
        let name = c_ident(&format!("release_array_t{}", elem.0));
        let es = elem_size(cx, elem);
        let mut b = FuncBuilder::new(CTy::Void, name);
        b.static_ = true;
        b.param(CTy::Ptr, "p");
        b.stmt(Stmt::decl(CTy::I32, "n", None));
        b.stmt(Stmt::decl(CTy::I32, "i", None));
        b.stmt(Stmt::if_(
            Expr::unary(UnOp::Not, Expr::id("p")),
            Stmt::Return(None),
        ));
        b.stmt(rc_header());
        maybe_defer(
            &mut b,
            &c_ident(&format!("destroy_array_t{}", elem.0)),
            mir.uses_defer,
        );
        let mut body = Vec::new();
        if interner.is_value_type(elem) {
            let at = Expr::cast(
                CTy::Ptr,
                Expr::add(
                    Expr::ptr_add(Expr::id("p"), super::types::len_prefix()),
                    Expr::mul(
                        Expr::cast(CTy::Named("size_t"), Expr::id("i")),
                        Expr::i(es as i64),
                    ),
                ),
            );
            body.extend(value_ref_stmts(cx, elem, at, false));
        } else if interner.is_rc_tracked(elem) {
            let rel = release_sym(interner, mir, elem);
            body.push(Stmt::call(
                rel,
                vec![Expr::load(
                    CTy::Ptr,
                    Expr::add(
                        Expr::ptr_add(Expr::id("p"), super::types::len_prefix()),
                        Expr::mul(
                            Expr::cast(CTy::Named("size_t"), Expr::id("i")),
                            Expr::i(es as i64),
                        ),
                    ),
                )],
            ));
        }
        b.stmt(Stmt::For {
            init: Box::new(Stmt::assign(Expr::id("i"), Expr::i(0))),
            cond: Expr::lt(Expr::id("i"), Expr::id("n")),
            step: Box::new(Stmt::Expr(Expr::PostInc(Box::new(Expr::id("i"))))),
            body: Box::new(Stmt::block(body)),
        });
        b.call("dream_free", vec![Expr::id("p")]);
        m.push_func(b);
    }
    for (_ty, layout) in &cx.native.structs {
        let name = c_ident(&format!("release_{}", layout.name));
        let mut b = FuncBuilder::new(CTy::Void, name);
        b.static_ = true;
        b.param(CTy::Ptr, "p");
        b.stmt(Stmt::if_(
            Expr::unary(UnOp::Not, Expr::id("p")),
            Stmt::Return(None),
        ));
        b.stmt(rc_header());
        maybe_defer(
            &mut b,
            &c_ident(&format!("destroy_{}", layout.name)),
            cx.mir.uses_defer,
        );
        if let Some(del) = cx
            .mir
            .functions
            .iter()
            .find(|f| f.name == format!("{}_del", layout.name))
        {
            b.stmt(Stmt::store(
                CTy::I32,
                Expr::add(Expr::char_p(Expr::id("p")), super::types::rc_delta()),
                Expr::i(1),
            ));
            b.call(c_ident(&func_symbol(del)), vec![Expr::id("p")]);
        }
        for f in &layout.fields {
            if f.is_weak || f.is_unowned {
                continue;
            }
            if interner.is_value_type(f.ty) {
                let at = Expr::cast(
                    CTy::Ptr,
                    Expr::ptr_add(Expr::id("p"), Expr::i(f.offset as i64)),
                );
                for s in value_ref_stmts(cx, f.ty, at, false) {
                    b.stmt(s);
                }
            } else if interner.is_rc_tracked(f.ty) {
                let rel = release_sym(interner, mir, f.ty);
                b.call(
                    rel,
                    vec![Expr::load(
                        CTy::Ptr,
                        Expr::ptr_add(Expr::id("p"), Expr::i(f.offset as i64)),
                    )],
                );
            }
        }
        b.call("dream_free", vec![Expr::id("p")]);
        m.push_func(b);
    }
    for (_ty, layout) in &cx.native.unions {
        let name = c_ident(&format!("release_{}", layout.name));
        let mut b = FuncBuilder::new(CTy::Void, name);
        b.static_ = true;
        b.param(CTy::Ptr, "p");
        b.stmt(Stmt::if_(
            Expr::unary(UnOp::Not, Expr::id("p")),
            Stmt::Return(None),
        ));
        b.stmt(rc_header());
        maybe_defer(
            &mut b,
            &c_ident(&format!("destroy_{}", layout.name)),
            cx.mir.uses_defer,
        );
        let mut arms = Vec::new();
        for variant in &layout.variants {
            let mut body = Vec::new();
            for field in &variant.fields {
                if field.is_weak || field.is_unowned {
                    continue;
                }
                if interner.is_value_type(field.ty) {
                    let at = Expr::cast(
                        CTy::Ptr,
                        Expr::ptr_add(Expr::id("p"), Expr::i(field.offset as i64)),
                    );
                    body.extend(value_ref_stmts(cx, field.ty, at, false));
                } else if interner.is_rc_tracked(field.ty) {
                    let rel = release_sym(interner, mir, field.ty);
                    body.push(Stmt::call(
                        rel,
                        vec![Expr::load(
                            CTy::Ptr,
                            Expr::ptr_add(Expr::id("p"), Expr::i(field.offset as i64)),
                        )],
                    ));
                }
            }
            body.push(Stmt::Expr(Expr::id("break")));
            arms.push(SwitchArm {
                keys: vec![CaseKey::Int(variant.discriminant as i64)],
                body,
            });
        }
        arms.push(SwitchArm {
            keys: vec![],
            body: vec![Stmt::Expr(Expr::id("break"))],
        });
        b.stmt(Stmt::Switch {
            expr: Expr::load(CTy::I32, Expr::dream_p(Expr::id("p"))),
            arms,
        });
        b.call("dream_free", vec![Expr::id("p")]);
        m.push_func(b);
    }
    emit_destroy_helpers(m, cx, &array_elems_copy);
}

fn emit_destroy_helpers(
    m: &mut ModuleBuilder,
    cx: &Cx<'_>,
    array_elems: &std::collections::BTreeSet<TypeId>,
) {
    let mir = cx.mir;
    let interner = cx.interner;
    let p = vec![Param {
        ty: CTy::Ptr,
        name: "p".into(),
    }];
    for elem in array_elems {
        m.static_proto(
            CTy::Void,
            c_ident(&format!("destroy_array_t{}", elem.0)),
            p.clone(),
        );
    }
    for layout in cx.native.structs.values() {
        m.static_proto(
            CTy::Void,
            c_ident(&format!("destroy_{}", layout.name)),
            p.clone(),
        );
    }
    for layout in cx.native.unions.values() {
        m.static_proto(
            CTy::Void,
            c_ident(&format!("destroy_{}", layout.name)),
            p.clone(),
        );
    }
    m.static_proto(CTy::Void, c_ident("destroy_object"), p.clone());
    for elem in array_elems {
        let name = c_ident(&format!("destroy_array_t{}", elem.0));
        let es = elem_size(cx, *elem);
        let mut b = FuncBuilder::new(CTy::Void, name.clone());
        b.static_ = true;
        b.param(CTy::Ptr, "p");
        b.stmt(Stmt::decl(CTy::I32, "n", None));
        b.stmt(Stmt::decl(CTy::I32, "i", None));
        b.stmt(Stmt::if_(
            Expr::unary(UnOp::Not, Expr::id("p")),
            Stmt::Return(None),
        ));
        b.stmt(unique_header());
        maybe_defer(&mut b, &name, cx.mir.uses_defer);
        b.assign(
            Expr::id("n"),
            Expr::load(CTy::I32, Expr::dream_p(Expr::id("p"))),
        );
        let mut body = Vec::new();
        if interner.is_value_type(*elem) {
            let at = Expr::cast(
                CTy::Ptr,
                Expr::add(
                    Expr::ptr_add(Expr::id("p"), super::types::len_prefix()),
                    Expr::mul(
                        Expr::cast(CTy::Named("size_t"), Expr::id("i")),
                        Expr::i(es as i64),
                    ),
                ),
            );
            body.extend(value_ref_stmts(cx, *elem, at, false));
        } else if interner.is_rc_tracked(*elem) {
            let rel = release_sym(interner, mir, *elem);
            body.push(Stmt::call(
                rel,
                vec![Expr::load(
                    CTy::Ptr,
                    Expr::add(
                        Expr::ptr_add(Expr::id("p"), super::types::len_prefix()),
                        Expr::mul(
                            Expr::cast(CTy::Named("size_t"), Expr::id("i")),
                            Expr::i(es as i64),
                        ),
                    ),
                )],
            ));
        }
        b.stmt(Stmt::For {
            init: Box::new(Stmt::assign(Expr::id("i"), Expr::i(0))),
            cond: Expr::lt(Expr::id("i"), Expr::id("n")),
            step: Box::new(Stmt::Expr(Expr::PostInc(Box::new(Expr::id("i"))))),
            body: Box::new(Stmt::block(body)),
        });
        b.call("dream_free", vec![Expr::id("p")]);
        m.push_func(b);
    }
    for (_ty, layout) in &cx.native.structs {
        let name = c_ident(&format!("destroy_{}", layout.name));
        let mut b = FuncBuilder::new(CTy::Void, name.clone());
        b.static_ = true;
        b.param(CTy::Ptr, "p");
        b.stmt(Stmt::if_(
            Expr::unary(UnOp::Not, Expr::id("p")),
            Stmt::Return(None),
        ));
        b.stmt(unique_header());
        maybe_defer(&mut b, &name, cx.mir.uses_defer);
        if let Some(del) = cx
            .mir
            .functions
            .iter()
            .find(|f| f.name == format!("{}_del", layout.name))
        {
            b.stmt(Stmt::store(
                CTy::I32,
                Expr::add(Expr::char_p(Expr::id("p")), super::types::rc_delta()),
                Expr::i(1),
            ));
            b.call(c_ident(&func_symbol(del)), vec![Expr::id("p")]);
        }
        for f in &layout.fields {
            if f.is_weak || f.is_unowned {
                continue;
            }
            if interner.is_value_type(f.ty) {
                let at = Expr::cast(
                    CTy::Ptr,
                    Expr::ptr_add(Expr::id("p"), Expr::i(f.offset as i64)),
                );
                for s in value_ref_stmts(cx, f.ty, at, false) {
                    b.stmt(s);
                }
            } else if interner.is_rc_tracked(f.ty) {
                let rel = release_sym(interner, mir, f.ty);
                b.call(
                    rel,
                    vec![Expr::load(
                        CTy::Ptr,
                        Expr::ptr_add(Expr::id("p"), Expr::i(f.offset as i64)),
                    )],
                );
            }
        }
        b.call("dream_free", vec![Expr::id("p")]);
        m.push_func(b);
    }
    for (_ty, layout) in &cx.native.unions {
        let name = c_ident(&format!("destroy_{}", layout.name));
        let mut b = FuncBuilder::new(CTy::Void, name.clone());
        b.static_ = true;
        b.param(CTy::Ptr, "p");
        b.stmt(Stmt::if_(
            Expr::unary(UnOp::Not, Expr::id("p")),
            Stmt::Return(None),
        ));
        b.stmt(unique_header());
        maybe_defer(&mut b, &name, cx.mir.uses_defer);
        let mut arms = Vec::new();
        for variant in &layout.variants {
            let mut body = Vec::new();
            for field in &variant.fields {
                if field.is_weak || field.is_unowned {
                    continue;
                }
                if interner.is_value_type(field.ty) {
                    let at = Expr::cast(
                        CTy::Ptr,
                        Expr::ptr_add(Expr::id("p"), Expr::i(field.offset as i64)),
                    );
                    body.extend(value_ref_stmts(cx, field.ty, at, false));
                } else if interner.is_rc_tracked(field.ty) {
                    let rel = release_sym(interner, mir, field.ty);
                    body.push(Stmt::call(
                        rel,
                        vec![Expr::load(
                            CTy::Ptr,
                            Expr::ptr_add(Expr::id("p"), Expr::i(field.offset as i64)),
                        )],
                    ));
                }
            }
            body.push(Stmt::Expr(Expr::id("break")));
            arms.push(SwitchArm {
                keys: vec![CaseKey::Int(variant.discriminant as i64)],
                body,
            });
        }
        arms.push(SwitchArm {
            keys: vec![],
            body: vec![Stmt::Expr(Expr::id("break"))],
        });
        b.stmt(Stmt::Switch {
            expr: Expr::load(CTy::I32, Expr::dream_p(Expr::id("p"))),
            arms,
        });
        b.call("dream_free", vec![Expr::id("p")]);
        m.push_func(b);
    }
    emit_destroy_object(m, cx);
}

fn emit_destroy_object(m: &mut ModuleBuilder, cx: &Cx<'_>) {
    let mut b = FuncBuilder::new(CTy::Void, c_ident("destroy_object"));
    b.static_ = true;
    b.param(CTy::Ptr, "p");
    b.stmt(Stmt::if_(
        Expr::unary(UnOp::Not, Expr::id("p")),
        Stmt::Return(None),
    ));
    maybe_defer(&mut b, "destroy_object", cx.mir.uses_defer);
    b.stmt(Stmt::decl(
        CTy::I32,
        "tag",
        Some(Expr::call("dream_object_tag", vec![Expr::id("p")])),
    ));
    let mut arms = Vec::new();
    for (ty, layout) in &cx.native.structs {
        if let Some(&tag) = cx.tags.get(ty) {
            arms.push(SwitchArm {
                keys: vec![CaseKey::Int(tag as i64)],
                body: vec![
                    Stmt::call(
                        c_ident(&format!("destroy_{}", layout.name)),
                        vec![Expr::id("p")],
                    ),
                    Stmt::Return(None),
                ],
            });
        }
    }
    for (ty, layout) in &cx.native.unions {
        if let Some(&tag) = cx.tags.get(ty) {
            arms.push(SwitchArm {
                keys: vec![CaseKey::Int(tag as i64)],
                body: vec![
                    Stmt::call(
                        c_ident(&format!("destroy_{}", layout.name)),
                        vec![Expr::id("p")],
                    ),
                    Stmt::Return(None),
                ],
            });
        }
    }
    arms.push(SwitchArm {
        keys: vec![CaseKey::Ident("TAG_STRING")],
        body: vec![
            Stmt::call("dream_release", vec![Expr::id("p")]),
            Stmt::Return(None),
        ],
    });
    arms.push(SwitchArm {
        keys: vec![],
        body: vec![Stmt::call("dream_release", vec![Expr::id("p")])],
    });
    b.stmt(Stmt::Switch {
        expr: Expr::id("tag"),
        arms,
    });
    m.push_func(b);
}
