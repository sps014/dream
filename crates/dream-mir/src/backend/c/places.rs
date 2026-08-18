use crate::backend::c::ctx::Cx;
use crate::backend::c::types::{array_elem_ty, elem_size, load_cast};
use crate::{Operand, Place};

pub(super) fn emit_operand(cx: &Cx<'_>, f: &crate::MirFunction, o: &Operand) -> String {
    match o {
        Operand::Copy(p) => emit_load(cx, f, p),
        Operand::Const(c) => match c {
            crate::Const::Int(v) => format!("{v}"),
            crate::Const::Long(v) => format!("{v}LL"),
            crate::Const::Float(v) => {
                if v.is_nan() {
                    "(double)NAN".into()
                } else if v.is_infinite() {
                    if *v > 0.0 {
                        "(double)INFINITY".into()
                    } else {
                        "(-(double)INFINITY)".into()
                    }
                } else {
                    format!("{v}")
                }
            }
            crate::Const::F32(v) => {
                if v.is_nan() {
                    "NAN".into()
                } else if v.is_infinite() {
                    if *v > 0.0 {
                        "INFINITY".into()
                    } else {
                        "(-INFINITY)".into()
                    }
                } else {
                    let mut s = format!("{v}");
                    if !s.contains('.') && !s.contains('e') && !s.contains('E') {
                        s.push_str(".0");
                    }
                    s.push('f');
                    s
                }
            }
            crate::Const::Bool(b) => if *b { "1" } else { "0" }.into(),
            crate::Const::Char(ch) => format!("{}", *ch as u32),
            crate::Const::Str(s) => cx.str_sym(s).to_string(),
            crate::Const::Null => "0".into(),
        },
    }
}

pub(super) fn emit_load(cx: &Cx<'_>, f: &crate::MirFunction, place: &Place) -> String {
    match place {
        Place::Local(l) => format!("l{}", l.0),
        Place::Global(g) => format!("g{}", g.0),
        Place::Field { base, field } => {
            let ty = f.local_ty(*base);
            let Some(layout) = cx.nstruct(ty) else {
                return format!("((dream_ptr*)l{})[{field}]", base.0);
            };
            let fld = layout
                .fields
                .get(*field)
                .unwrap_or_else(|| crate::internal_error!("missing field {field} on type {ty:?}"));
            if cx.interner.is_value_type(fld.ty) {
                return format!(
                    "((dream_ptr)((char*)dream_p(l{}) + {}))",
                    base.0, fld.offset
                );
            }
            let cast = load_cast(cx, fld.ty);
            let load = format!("(*({cast}*)((char*)dream_p(l{}) + {}))", base.0, fld.offset);
            if fld.is_unowned {
                let panic = cx.str_sym(crate::backend::wasm::panic_msgs::UNOWNED_NULL_DEREF);
                format!(
                    "({{ {cast} __unowned = {load}; if (!__unowned) dream_panic({panic}); __unowned; }})"
                )
            } else {
                load
            }
        }
        Place::Index {
            base,
            index,
            unchecked,
        } => {
            let ety = array_elem_ty(cx.interner, f.local_ty(*base));
            let addr = index_addr(cx, f, *base, index, elem_size(cx, ety), *unchecked);
            if cx.interner.is_value_type(ety) {
                return format!("((dream_ptr)({addr}))");
            }
            let cast = load_cast(cx, ety);
            format!("(*({cast}*)({addr}))")
        }
        Place::Deref { ptr, elem_ty } => {
            if cx.interner.is_value_type(*elem_ty) {
                return format!("l{}", ptr.0);
            }
            let cast = load_cast(cx, *elem_ty);
            format!("(*({cast}*)dream_p(l{}))", ptr.0)
        }
    }
}

fn realloc_self_store(place: &Place, rv: &crate::Rvalue) -> bool {
    let crate::Rvalue::ArrayRealloc { array, .. } = rv else {
        return false;
    };
    let Operand::Copy(src) = array else {
        return false;
    };
    match (place, src) {
        (
            Place::Field {
                base: b1,
                field: f1,
            },
            Place::Field {
                base: b2,
                field: f2,
            },
        ) => b1 == b2 && f1 == f2,
        (Place::Local(l1), Place::Local(l2)) => l1 == l2,
        (Place::Global(g1), Place::Global(g2)) => g1 == g2,
        _ => false,
    }
}

pub(super) fn emit_store(
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    place: &Place,
    rv: &crate::Rvalue,
    rhs: &str,
) -> String {
    match place {
        Place::Local(l) if cx.interner.is_value_type(f.local_ty(*l)) => {
            if is_value_place_alias(f, *l, rv) {
                return format!("l{} = (dream_ptr)({rhs})", l.0);
            }
            if let crate::Rvalue::Use(Operand::Copy(Place::Local(src))) = rv {
                let src_ty = f.local_ty(*src);
                if !cx.interner.is_value_type(src_ty)
                    && cx.nstruct(src_ty).is_some_and(|layout| {
                        layout.size == elem_size(cx, f.local_ty(*l))
                    })
                {
                    return format!("l{} = l{}", l.0, src.0);
                }
            }
            let retain_copy = matches!(
                rv,
                crate::Rvalue::Use(Operand::Copy(Place::Field { .. }))
                    | crate::Rvalue::Use(Operand::Copy(Place::Index { .. }))
                    | crate::Rvalue::Use(Operand::Copy(Place::Deref { .. }))
                    | crate::Rvalue::UnionField { .. }
            );
            memcpy_value(
                cx,
                rv,
                rhs,
                f.local_ty(*l),
                &format!("dream_p(l{})", l.0),
                &format!("l{}", l.0),
                retain_copy,
            )
        }
        Place::Local(l) => format!("l{} = ({rhs})", l.0),
        Place::Global(g) => {
            let value_ty = cx
                .mir
                .globals
                .iter()
                .find(|global| global.id == *g)
                .map(|global| global.ty)
                .filter(|ty| cx.interner.is_value_type(*ty));
            if let Some(ty) = value_ty {
                let size = elem_size(cx, ty);
                format!("memcpy(dream_p(g{}), dream_p({rhs}), {size})", g.0)
            } else {
                format!("g{} = ({rhs})", g.0)
            }
        }
        Place::Field { base, field } => {
            let ty = f.local_ty(*base);
            let Some(layout) = cx.nstruct(ty) else {
                return format!("((dream_ptr*)l{})[{field}] = (dream_ptr)({rhs})", base.0);
            };
            let fld = layout
                .fields
                .get(*field)
                .unwrap_or_else(|| crate::internal_error!("missing field {field} on type {ty:?}"));
            if cx.interner.is_value_type(fld.ty) {
                return memcpy_value(
                    cx,
                    rv,
                    rhs,
                    fld.ty,
                    &format!("(char*)dream_p(l{}) + {}", base.0, fld.offset),
                    &format!("((dream_ptr)((char*)dream_p(l{}) + {}))", base.0, fld.offset),
                    true,
                );
            }
            let cast = load_cast(cx, fld.ty);
            if realloc_self_store(place, rv) {
                return format!(
                    "*({cast}*)((char*)dream_p(l{}) + {}) = ({cast})({rhs})",
                    base.0, fld.offset
                );
            }
            if fld.is_unowned {
                let slot = format!("((char*)dream_p(l{}) + {})", base.0, fld.offset);
                return format!(
                    "({{ dream_ptr __old = *(dream_ptr*){slot}; if (__old) dream_weak_unregister(__old, (dream_ptr){slot}); dream_ptr __new = (dream_ptr)({rhs}); *(dream_ptr*){slot} = __new; if (__new) dream_weak_register(__new, (dream_ptr){slot}, 1, 0); }})"
                );
            }
            if fld.is_weak {
                return emit_weak_option_store(cx, *base, fld, rv, rhs);
            }
            if cx.interner.is_reference(fld.ty) && !fld.is_weak {
                let release = crate::backend::c::release::release_sym(cx.interner, cx.mir, fld.ty);
                let slot = format!("((char*)dream_p(l{}) + {})", base.0, fld.offset);
                if borrowed_ref_store(rv) {
                    return format!(
                        "({{ dream_ptr __old = *(dream_ptr*){slot}; dream_ptr __v = (dream_ptr)({rhs}); if (__old != __v) {{ dream_retain(__v); *({cast}*){slot} = ({cast})__v; {release}(__old); }} }})"
                    );
                }
                return format!(
                    "({{ dream_ptr __old = *(dream_ptr*){slot}; dream_ptr __v = (dream_ptr)({rhs}); *({cast}*){slot} = ({cast})__v; {release}(__old); }})"
                );
            }
            format!(
                "*({cast}*)((char*)dream_p(l{}) + {}) = ({cast})({rhs})",
                base.0, fld.offset
            )
        }
        Place::Index {
            base,
            index,
            unchecked,
        } => {
            let ety = array_elem_ty(cx.interner, f.local_ty(*base));
            let es = elem_size(cx, ety);
            let addr = index_addr(cx, f, *base, index, es, *unchecked);
            if cx.interner.is_value_type(ety) {
                return format!("memcpy({addr}, dream_p({rhs}), {es})",);
            }
            let cast = load_cast(cx, ety);
            if realloc_self_store(place, rv) {
                return format!("*({cast}*)({addr}) = ({cast})({rhs})");
            }
            if cx.interner.is_reference(ety) {
                let release = crate::backend::c::release::release_sym(cx.interner, cx.mir, ety);
                if borrowed_ref_store(rv) {
                    return format!(
                        "({{ dream_ptr __old = *(dream_ptr*)({addr}); dream_ptr __v = (dream_ptr)({rhs}); if (__old != __v) {{ dream_retain(__v); *({cast}*)({addr}) = ({cast})__v; {release}(__old); }} }})",
                    );
                }
                return format!(
                    "({{ dream_ptr __old = *(dream_ptr*)({addr}); dream_ptr __v = (dream_ptr)({rhs}); *({cast}*)({addr}) = ({cast})__v; {release}(__old); }})",
                );
            }
            format!("*({cast}*)({addr}) = ({cast})({rhs})")
        }
        Place::Deref { ptr, elem_ty } => {
            if cx.interner.is_value_type(*elem_ty) {
                let size = elem_size(cx, *elem_ty);
                return format!("memcpy(dream_p(l{}), dream_p({rhs}), {size})", ptr.0);
            }
            let cast = load_cast(cx, *elem_ty);
            format!("*({cast}*)dream_p(l{}) = ({cast})({rhs})", ptr.0)
        }
    }
}

fn index_addr(
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    base: crate::Local,
    index: &Operand,
    elem_size: u32,
    unchecked: bool,
) -> String {
    let idx = emit_operand(cx, f, index);
    if cx.omit_bounds
        || unchecked
        || !matches!(
            cx.interner.kind(f.local_ty(base)),
            dream_types::TyKind::Array(_)
        )
    {
        return format!("((char*)dream_p(l{}) + 4 + ({idx})*{elem_size})", base.0);
    }
    // Native `int` temps are i64 so IV/pointer values must not be treated as indices.
    // Only trap when the index is a 32-bit in-range value (user `arr[i]`), matching wasm i32.
    let panic = cx.str_sym(crate::backend::wasm::panic_msgs::INDEX_OUT_OF_BOUNDS);
    format!(
        "({{ int32_t __idx = (int32_t)({idx}); int32_t __len = l{} ? *(int32_t*)dream_p(l{}) : 0; if ((int64_t)({idx}) == (int64_t)__idx && (uint32_t)__idx >= (uint32_t)__len) dream_panic({panic}); (char*)dream_p(l{}) + 4 + (int64_t)({idx})*{elem_size}; }})",
        base.0, base.0, base.0
    )
}

fn memcpy_value(
    cx: &Cx<'_>,
    rv: &crate::Rvalue,
    rhs: &str,
    ty: dream_types::TypeId,
    dest: &str,
    dest_ptr: &str,
    retain_copy: bool,
) -> String {
    let size = elem_size(cx, ty);
    let mut s = if value_rvalue_allocates(rv) {
        format!("({{ dream_ptr __v = {rhs}; memcpy({dest}, dream_p(__v), {size}); dream_free(__v); ")
    } else {
        format!("memcpy({dest}, dream_p({rhs}), {size}); ")
    };
    if retain_copy && !value_rvalue_allocates(rv) {
        crate::backend::c::statements::emit_value_refs(&mut s, cx, ty, dest_ptr, true);
    }
    if value_rvalue_allocates(rv) {
        s.push_str("})");
    }
    s
}

fn borrowed_ref_store(rv: &crate::Rvalue) -> bool {
    !value_rvalue_allocates(rv)
}

fn value_rvalue_allocates(rv: &crate::Rvalue) -> bool {
    matches!(
        rv,
        crate::Rvalue::New { .. }
            | crate::Rvalue::Tuple { .. }
            | crate::Rvalue::UnionNew { .. }
            | crate::Rvalue::Call { .. }
            | crate::Rvalue::InterfaceCall { .. }
            | crate::Rvalue::IndirectCall { .. }
            | crate::Rvalue::ArrayLit { .. }
            | crate::Rvalue::ArrayNew { .. }
            | crate::Rvalue::ArrayRealloc { .. }
    )
}

pub(super) fn is_alias_value_local(f: &crate::MirFunction, local: crate::Local) -> bool {
    if f.locals[local.0 as usize].name.is_some() {
        return false;
    }
    let mut seen = false;
    for stmt in f.blocks.iter().flat_map(|block| &block.stmts) {
        let crate::Statement::Assign(crate::Place::Local(other), rv) = stmt else {
            continue;
        };
        if *other != local {
            continue;
        }
        seen = true;
        if !is_value_place_alias(f, local, rv) {
            return false;
        }
    }
    seen
}

pub(super) fn is_value_copy_local(f: &crate::MirFunction, local: crate::Local) -> bool {
    let mut seen = false;
    for stmt in f.blocks.iter().flat_map(|block| &block.stmts) {
        let crate::Statement::Assign(crate::Place::Local(other), rv) = stmt else {
            continue;
        };
        if *other != local {
            continue;
        }
        seen = true;
        if !matches!(rv, crate::Rvalue::Use(_)) {
            return false;
        }
    }
    seen
}

pub(super) fn is_moved_into_union(f: &crate::MirFunction, local: crate::Local) -> bool {
    f.blocks.iter().flat_map(|block| &block.stmts).any(|stmt| {
        let crate::Statement::Assign(_, rv) = stmt else {
            return false;
        };
        let crate::Rvalue::UnionNew { args, .. } = rv else {
            return false;
        };
        args.iter().any(|arg| {
            matches!(arg, crate::Operand::Copy(crate::Place::Local(src)) if *src == local)
        })
    })
}

pub(super) fn is_value_place_alias(
    f: &crate::MirFunction,
    local: crate::Local,
    rv: &crate::Rvalue,
) -> bool {
    if f.locals[local.0 as usize].name.is_some() {
        return false;
    }
    let place_ok = matches!(
        rv,
        crate::Rvalue::Use(Operand::Copy(Place::Local(_)))
            | crate::Rvalue::Use(Operand::Copy(Place::Index { .. }))
            | crate::Rvalue::Use(Operand::Copy(Place::Field { .. }))
            | crate::Rvalue::Use(Operand::Copy(Place::Deref { .. }))
            | crate::Rvalue::UnionField { .. }
    );
    if !place_ok {
        return false;
    }
    f.blocks.iter().flat_map(|block| &block.stmts).all(|stmt| {
        !matches!(
            stmt,
            crate::Statement::Assign(Place::Local(other), _) if *other == local
        ) || matches!(
            stmt,
            crate::Statement::Assign(
                Place::Local(other),
                crate::Rvalue::Use(Operand::Copy(Place::Local(_)))
                    | crate::Rvalue::Use(Operand::Copy(Place::Index { .. }))
                    | crate::Rvalue::Use(Operand::Copy(Place::Field { .. }))
                    | crate::Rvalue::Use(Operand::Copy(Place::Deref { .. }))
                    | crate::Rvalue::UnionField { .. }
            ) if *other == local
        )
    })
}

fn emit_weak_option_store(
    cx: &Cx<'_>,
    base: crate::Local,
    fld: &dream_hir::FieldLayout,
    rv: &crate::Rvalue,
    rhs: &str,
) -> String {
    let Some(u) = cx.nunion(fld.ty) else {
        let slot = format!("((char*)dream_p(l{}) + {})", base.0, fld.offset);
        return format!("*(dream_ptr*){slot} = (dream_ptr)({rhs})");
    };
    let some = u.variant("Some").map(|v| v.discriminant).unwrap_or(0);
    let none = u.variant("None").map(|v| v.discriminant).unwrap_or(1);
    let poff = u
        .variant("Some")
        .and_then(|v| v.fields.first())
        .map(|f| f.offset)
        .unwrap_or(8);
    let size = u.size.max(16);
    let slot = format!("((char*)dream_p(l{}) + {})", base.0, fld.offset);
    let drop_src = if value_rvalue_allocates(rv) {
        let rel = crate::backend::c::release::release_sym(cx.interner, cx.mir, fld.ty);
        format!("{rel}(__src); ")
    } else {
        String::new()
    };
    format!(
        "({{ dream_ptr __src = (dream_ptr)({rhs}); dream_ptr __old = *(dream_ptr*){slot}; dream_ptr __box = dream_malloc({size}, 0); memcpy(dream_p(__box), dream_p(__src), {size}); if (*(int32_t*)dream_p(__src) == {some}) {{ dream_weak_register(*(dream_ptr*)((char*)dream_p(__src) + {poff}), __box, 0, (dream_ptr)(intptr_t){none}); }} *(dream_ptr*){slot} = __box; {drop_src}if (__old) {{ if (*(int32_t*)dream_p(__old) == {some}) dream_weak_unregister(*(dream_ptr*)((char*)dream_p(__old) + {poff}), __old); dream_free(__old); }} }})"
    )
}
