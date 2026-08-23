use super::ast::{CTy, CaseKey, Expr, Param, Stmt, SwitchArm, UnOp};
use super::builder::{FuncBuilder, ModuleBuilder};
use super::ctx::Cx;
use super::statements::value_ref_stmts;
use super::types::{c_ident, elem_size};
use crate::backend::shared::func_symbol;
use dream_types::{TyKind, TypeId, TypeInterner};

pub(super) fn retain_sym(cx: &Cx<'_>, ty: TypeId) -> &'static str {
    if cx.target.is_wasm32() && matches!(cx.interner.kind(ty), TyKind::Js) {
        "js_retain"
    } else {
        "dream_retain"
    }
}

pub(super) fn release_sym(cx: &Cx<'_>, ty: TypeId) -> String {
    let interner = cx.interner;
    let mir = cx.mir;
    match interner.kind(ty) {
        TyKind::Js if cx.target.is_wasm32() => "js_release".into(),
        TyKind::Struct(..) | TyKind::Union(..) => {
            let raw = if let Some(l) = mir.layouts.structs.get(&ty) {
                c_ident(&format!("release_{}", l.name))
            } else if let Some(l) = mir.layouts.unions.get(&ty) {
                c_ident(&format!("release_{}", l.name))
            } else {
                return "dream_release".into();
            };
            cx.canon_maps().release.get(&ty).cloned().unwrap_or(raw)
        }
        TyKind::Array(e) if interner.is_reference(*e) || interner.is_value_type(*e) => {
            let raw = c_ident(&format!("release_array_t{}", e.0));
            cx.canon_maps().release.get(e).cloned().unwrap_or(raw)
        }
        TyKind::Func(..) => "dream_release_funcbox".into(),
        TyKind::Prim(dream_types::PrimTy::String) if mir.uses_defer => "release_string".into(),
        _ => "dream_release".into(),
    }
}

pub(super) fn destroy_sym(cx: &Cx<'_>, ty: TypeId) -> String {
    if matches!(cx.interner.kind(ty), TyKind::Js | TyKind::Func(..))
        || cx.interner.is_shared_type(ty)
        || matches!(cx.interner.kind(ty), TyKind::Prim(dream_types::PrimTy::String))
    {
        return release_sym(cx, ty);
    }
    match cx.interner.kind(ty) {
        TyKind::Struct(..) | TyKind::Union(..) => {
            let raw = if let Some(l) = cx.mir.layouts.structs.get(&ty) {
                c_ident(&format!("destroy_{}", l.name))
            } else if let Some(l) = cx.mir.layouts.unions.get(&ty) {
                c_ident(&format!("destroy_{}", l.name))
            } else {
                return c_ident("destroy_object");
            };
            cx.canon_maps().destroy.get(&ty).cloned().unwrap_or(raw)
        }
        TyKind::Array(e) if cx.interner.is_reference(*e) || cx.interner.is_value_type(*e) => {
            let raw = c_ident(&format!("destroy_array_t{}", e.0));
            cx.canon_maps().destroy.get(e).cloned().unwrap_or(raw)
        }
        TyKind::Object | TyKind::Interface(..) => c_ident("destroy_object"),
        _ => "dream_destroy".into(),
    }
}

/// Symbol canonicalization for ARC glue: types whose `release_*` (resp.
/// `destroy_*`) bodies would be byte-identical share one emitted function —
/// fieldless structs, arrays of plain references, and so on. Maps contain only
/// *redirects*: entries whose target differs from the type's own raw symbol,
/// keyed by struct/union id resp. array element id.
pub(super) struct CanonMaps {
    pub release: std::collections::HashMap<TypeId, String>,
    pub destroy: std::collections::HashMap<TypeId, String>,
}

pub(super) fn canonical_maps(cx: &Cx<'_>) -> CanonMaps {
    let mut rel: Vec<(String, TypeId, String)> = Vec::new();
    let mut des: Vec<(String, TypeId, String)> = Vec::new();
    for (ty, layout) in &cx.native.structs {
        if has_del(cx, &layout.name) {
            continue;
        }
        let Some(key) = struct_profile_key(cx, *ty) else {
            continue;
        };
        rel.push((format!("S|rel|{key}"), *ty, c_ident(&format!("release_{}", layout.name))));
        des.push((format!("S|des|{key}"), *ty, c_ident(&format!("destroy_{}", layout.name))));
    }
    for (ty, layout) in &cx.native.unions {
        let Some(key) = union_profile_key(cx, *ty) else {
            continue;
        };
        rel.push((format!("U|rel|{key}"), *ty, c_ident(&format!("release_{}", layout.name))));
        des.push((format!("U|des|{key}"), *ty, c_ident(&format!("destroy_{}", layout.name))));
    }
    // Arrays are excluded from dedup: their bodies embed the element type's own
    // release symbol, so identical-looking shapes (a plain ref union vs string)
    // still differ. The empty-loop skip below already minimizes the trivial ones.
    CanonMaps {
        release: redirects(rel),
        destroy: redirects(des),
    }
}

/// Group candidates by body-shape key and pick the lexically smallest symbol in
/// each group as the representative (BTreeMap + sort keep output deterministic).
fn redirects(cands: Vec<(String, TypeId, String)>) -> std::collections::HashMap<TypeId, String> {
    let mut groups: std::collections::BTreeMap<String, Vec<(TypeId, String)>> =
        std::collections::BTreeMap::new();
    for (key, ty, sym) in cands {
        groups.entry(key).or_default().push((ty, sym));
    }
    let mut out = std::collections::HashMap::new();
    for (_, mut members) in groups {
        members.sort_by(|a, b| a.1.cmp(&b.1));
        let rep = members[0].1.clone();
        for (ty, sym) in members {
            if sym != rep {
                out.insert(ty, rep.clone());
            }
        }
    }
    out
}

fn has_del(cx: &Cx<'_>, name: &str) -> bool {
    cx.mir
        .functions
        .iter()
        .any(|f| f.name == format!("{name}_del"))
}

/// Body key for a struct's glue: everything the emitted code can depend on
/// (field offsets, types, ownership flags). `None` = not dedupable.
fn struct_profile_key(cx: &Cx<'_>, ty: TypeId) -> Option<String> {
    let layout = cx.native.structs.get(&ty)?;
    let mut key = String::new();
    for f in &layout.fields {
        key.push_str(&format!("|{}:{}", f.offset, f.ty.0));
        if f.is_weak {
            key.push('w');
        } else if f.is_unowned {
            key.push('u');
        } else if cx.interner.is_value_type(f.ty) {
            key.push('v');
        } else if cx.interner.is_rc_tracked(f.ty) {
            key.push('r');
        }
    }
    Some(key)
}

fn union_profile_key(cx: &Cx<'_>, ty: TypeId) -> Option<String> {
    let layout = cx.native.unions.get(&ty)?;
    let mut key = String::new();
    for v in &layout.variants {
        key.push_str(&format!("|d{}", v.discriminant));
        for f in &v.fields {
            key.push_str(&format!(",{}:{}", f.offset, f.ty.0));
            if f.is_weak {
                key.push('w');
            } else if f.is_unowned {
                key.push('u');
            } else if cx.interner.is_value_type(f.ty) {
                key.push('v');
            } else if cx.interner.is_rc_tracked(f.ty) {
                key.push('r');
            }
        }
    }
    Some(key)
}

fn rc_header() -> Stmt {
    Stmt::if_(
        Expr::unary(
            UnOp::Not,
            Expr::call("dream_rc_last", vec![Expr::id("p")]),
        ),
        Stmt::Return(None),
    )
}

fn unique_header() -> Stmt {
    Stmt::if_(
        Expr::call("dream_rc_immortal", vec![Expr::id("p")]),
        Stmt::Return(None),
    )
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
    let canon = cx.canon_maps();
    let rel_redirect = |ty: &TypeId| canon.release.contains_key(ty);
    for elem in &array_elems {
        if rel_redirect(elem) {
            continue;
        }
        m.static_proto(
            CTy::Void,
            c_ident(&format!("release_array_t{}", elem.0)),
            p.clone(),
        );
    }
    for (ty, layout) in &cx.native.structs {
        if rel_redirect(ty) {
            continue;
        }
        m.static_proto(
            CTy::Void,
            c_ident(&format!("release_{}", layout.name)),
            p.clone(),
        );
    }
    for (ty, layout) in &cx.native.unions {
        if rel_redirect(ty) {
            continue;
        }
        m.static_proto(
            CTy::Void,
            c_ident(&format!("release_{}", layout.name)),
            p.clone(),
        );
    }
    if mir.uses_defer {
        let des_redirect = |ty: &TypeId| canon.destroy.contains_key(ty);
        for elem in &array_elems {
            if des_redirect(elem) {
                continue;
            }
            m.static_proto(
                CTy::Void,
                c_ident(&format!("destroy_array_t{}", elem.0)),
                p.clone(),
            );
        }
        for (ty, layout) in &cx.native.structs {
            if des_redirect(ty) {
                continue;
            }
            m.static_proto(
                CTy::Void,
                c_ident(&format!("destroy_{}", layout.name)),
                p.clone(),
            );
        }
        for (ty, layout) in &cx.native.unions {
            if des_redirect(ty) {
                continue;
            }
            m.static_proto(
                CTy::Void,
                c_ident(&format!("destroy_{}", layout.name)),
                p.clone(),
            );
        }
        m.static_proto(CTy::Void, c_ident("destroy_object"), p.clone());
        m.static_proto(CTy::Void, "release_string", p.clone());
        m.static_proto(CTy::Void, "destroy_string", p.clone());
    }
    let array_elems_copy = array_elems.clone();
    for elem in array_elems {
        if rel_redirect(&elem) {
            continue;
        }
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
        b.assign(
            Expr::id("n"),
            Expr::load(CTy::I32, Expr::dream_p(Expr::id("p"))),
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
            let rel = release_sym(cx, elem);
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
        if !body.is_empty() {
            b.stmt(Stmt::For {
                init: Box::new(Stmt::assign(Expr::id("i"), Expr::i(0))),
                cond: Expr::lt(Expr::id("i"), Expr::id("n")),
                step: Box::new(Stmt::Expr(Expr::PostInc(Box::new(Expr::id("i"))))),
                body: Box::new(Stmt::block(body)),
            });
        }
        b.call("dream_free", vec![Expr::id("p")]);
        m.push_func(b);
    }
    for (ty, layout) in &cx.native.structs {
        if rel_redirect(ty) {
            continue;
        }
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
            if f.is_weak {
                continue;
            }
            if f.is_unowned {
                // Unowned fields live in the weak registry (registered on store, see
                // `unowned_store`). Destroying the holder without unregistering leaves a
                // (target, slot) entry whose slot points into this freed block — a later
                // clear of the target would then write into freed memory.
                let slot = Expr::cast(
                    CTy::Ptr,
                    Expr::ptr_add(Expr::id("p"), Expr::i(f.offset as i64)),
                );
                let cur = Expr::load(CTy::Ptr, slot.clone());
                b.stmt(Stmt::if_(
                    cur.clone(),
                    Stmt::call(
                        "dream_weak_unregister",
                        vec![cur, Expr::cast(CTy::Ptr, slot)],
                    ),
                ));
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
                let rel = release_sym(cx, f.ty);
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
    for (ty, layout) in &cx.native.unions {
        if rel_redirect(ty) {
            continue;
        }
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
                    let rel = release_sym(cx, field.ty);
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
    if mir.uses_defer {
        emit_release_string(m);
    }
    emit_destroy_helpers(m, cx, &array_elems_copy);
}

fn emit_release_string(m: &mut ModuleBuilder) {
    let mut b = FuncBuilder::new(CTy::Void, "release_string");
    b.static_ = true;
    b.param(CTy::Ptr, "p");
    b.stmt(Stmt::if_(
        Expr::unary(UnOp::Not, Expr::id("p")),
        Stmt::Return(None),
    ));
    b.stmt(rc_header());
    maybe_defer(&mut b, "destroy_string", true);
    b.call("dream_free", vec![Expr::id("p")]);
    m.push_func(b);
}

fn emit_destroy_string(m: &mut ModuleBuilder) {
    let mut b = FuncBuilder::new(CTy::Void, "destroy_string");
    b.static_ = true;
    b.param(CTy::Ptr, "p");
    b.stmt(Stmt::if_(
        Expr::unary(UnOp::Not, Expr::id("p")),
        Stmt::Return(None),
    ));
    b.call("dream_free", vec![Expr::id("p")]);
    m.push_func(b);
}

fn emit_destroy_helpers(
    m: &mut ModuleBuilder,
    cx: &Cx<'_>,
    array_elems: &std::collections::BTreeSet<TypeId>,
) {
    let interner = cx.interner;
    let canon = cx.canon_maps();
    let des_redirect = |ty: &TypeId| canon.destroy.contains_key(ty);
    let p = vec![Param {
        ty: CTy::Ptr,
        name: "p".into(),
    }];
    for elem in array_elems {
        if des_redirect(elem) {
            continue;
        }
        m.static_proto(
            CTy::Void,
            c_ident(&format!("destroy_array_t{}", elem.0)),
            p.clone(),
        );
    }
    for (ty, layout) in &cx.native.structs {
        if des_redirect(ty) {
            continue;
        }
        m.static_proto(
            CTy::Void,
            c_ident(&format!("destroy_{}", layout.name)),
            p.clone(),
        );
    }
    for (ty, layout) in &cx.native.unions {
        if des_redirect(ty) {
            continue;
        }
        m.static_proto(
            CTy::Void,
            c_ident(&format!("destroy_{}", layout.name)),
            p.clone(),
        );
    }
    m.static_proto(CTy::Void, c_ident("destroy_object"), p.clone());
    for elem in array_elems {
        if des_redirect(elem) {
            continue;
        }
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
            let rel = release_sym(cx, *elem);
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
        if !body.is_empty() {
            b.stmt(Stmt::For {
                init: Box::new(Stmt::assign(Expr::id("i"), Expr::i(0))),
                cond: Expr::lt(Expr::id("i"), Expr::id("n")),
                step: Box::new(Stmt::Expr(Expr::PostInc(Box::new(Expr::id("i"))))),
                body: Box::new(Stmt::block(body)),
            });
        }
        b.call("dream_free", vec![Expr::id("p")]);
        m.push_func(b);
    }
    for (ty, layout) in &cx.native.structs {
        if des_redirect(ty) {
            continue;
        }
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
            if f.is_weak {
                continue;
            }
            if f.is_unowned {
                // Unowned fields live in the weak registry (registered on store, see
                // `unowned_store`). Destroying the holder without unregistering leaves a
                // (target, slot) entry whose slot points into this freed block — a later
                // clear of the target would then write into freed memory.
                let slot = Expr::cast(
                    CTy::Ptr,
                    Expr::ptr_add(Expr::id("p"), Expr::i(f.offset as i64)),
                );
                let cur = Expr::load(CTy::Ptr, slot.clone());
                b.stmt(Stmt::if_(
                    cur.clone(),
                    Stmt::call(
                        "dream_weak_unregister",
                        vec![cur, Expr::cast(CTy::Ptr, slot)],
                    ),
                ));
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
                let rel = release_sym(cx, f.ty);
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
    for (ty, layout) in &cx.native.unions {
        if des_redirect(ty) {
            continue;
        }
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
                    let rel = release_sym(cx, field.ty);
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
    if cx.mir.uses_defer {
        emit_destroy_string(m);
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
    for (ty, _layout) in &cx.native.structs {
        if let Some(&tag) = cx.tags.get(ty) {
            arms.push(SwitchArm {
                keys: vec![CaseKey::Int(tag as i64)],
                body: vec![
                    Stmt::call(destroy_sym(cx, *ty), vec![Expr::id("p")]),
                    Stmt::Return(None),
                ],
            });
        }
    }
    for (ty, _layout) in &cx.native.unions {
        if let Some(&tag) = cx.tags.get(ty) {
            arms.push(SwitchArm {
                keys: vec![CaseKey::Int(tag as i64)],
                body: vec![
                    Stmt::call(destroy_sym(cx, *ty), vec![Expr::id("p")]),
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
