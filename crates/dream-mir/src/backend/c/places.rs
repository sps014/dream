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
            let fld = layout.fields.get(*field).unwrap_or_else(|| {
                crate::internal_error!("missing field {field} on type {ty:?}")
            });
            if cx.interner.is_value_type(fld.ty) {
                return format!(
                    "((dream_ptr)((char*)dream_p(l{}) + {}))",
                    base.0, fld.offset
                );
            }
            let cast = load_cast(cx, fld.ty);
            format!(
                "(*({cast}*)((char*)dream_p(l{}) + {}))",
                base.0, fld.offset
            )
        }
        Place::Index { base, index, .. } => {
            let ety = array_elem_ty(cx.interner, f.local_ty(*base));
            let es = elem_size(cx, ety);
            if cx.interner.is_value_type(ety) {
                return format!(
                    "((dream_ptr)((char*)dream_p(l{}) + 4 + ({})*{es}))",
                    base.0,
                    emit_operand(cx, f, index)
                );
            }
            let cast = load_cast(cx, ety);
            format!(
                "(*({cast}*)((char*)dream_p(l{}) + 4 + ({})*{es}))",
                base.0,
                emit_operand(cx, f, index)
            )
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

pub(super) fn emit_store(
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    place: &Place,
    rv: &crate::Rvalue,
    rhs: &str,
) -> String {
    match place {
        Place::Local(l) if cx.interner.is_value_type(f.local_ty(*l)) => {
            if is_value_index_alias(f, *l, rv) {
                return format!("l{} = (dream_ptr)({rhs})", l.0);
            }
            let size = elem_size(cx, f.local_ty(*l));
            if value_rvalue_allocates(rv) {
                return format!(
                    "({{ dream_ptr __v = {rhs}; memcpy(dream_p(l{}), dream_p(__v), {size}); dream_free(__v); }})",
                    l.0
                );
            }
            format!("memcpy(dream_p(l{}), dream_p({rhs}), {size})", l.0)
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
            let fld = layout.fields.get(*field).unwrap_or_else(|| {
                crate::internal_error!("missing field {field} on type {ty:?}")
            });
            if cx.interner.is_value_type(fld.ty) {
                let size = elem_size(cx, fld.ty);
                return format!(
                    "memcpy((char*)dream_p(l{}) + {}, dream_p({rhs}), {size})",
                    base.0, fld.offset
                );
            }
            let cast = load_cast(cx, fld.ty);
            format!(
                "*({cast}*)((char*)dream_p(l{}) + {}) = ({cast})({rhs})",
                base.0, fld.offset
            )
        }
        Place::Index { base, index, .. } => {
            let ety = array_elem_ty(cx.interner, f.local_ty(*base));
            let es = elem_size(cx, ety);
            if cx.interner.is_value_type(ety) {
                return format!(
                    "memcpy((char*)dream_p(l{}) + 4 + ({})*{es}, dream_p({rhs}), {es})",
                    base.0,
                    emit_operand(cx, f, index)
                );
            }
            let cast = load_cast(cx, ety);
            if cx.interner.is_reference(ety) {
                return format!(
                    "({{ dream_ptr __v = (dream_ptr)({rhs}); dream_retain(__v); *({cast}*)((char*)dream_p(l{}) + 4 + ({})*{es}) = ({cast})__v; }})",
                    base.0,
                    emit_operand(cx, f, index)
                );
            }
            format!(
                "*({cast}*)((char*)dream_p(l{}) + 4 + ({})*{es}) = ({cast})({rhs})",
                base.0,
                emit_operand(cx, f, index)
            )
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

fn value_rvalue_allocates(rv: &crate::Rvalue) -> bool {
    matches!(
        rv,
        crate::Rvalue::New { .. } | crate::Rvalue::Tuple { .. } | crate::Rvalue::UnionNew { .. }
    )
}

pub(super) fn is_value_index_alias(
    f: &crate::MirFunction,
    local: crate::Local,
    rv: &crate::Rvalue,
) -> bool {
    if f.locals[local.0 as usize].name.is_some()
        || !matches!(rv, crate::Rvalue::Use(Operand::Copy(Place::Index { .. })))
    {
        return false;
    }
    f.blocks.iter().flat_map(|block| &block.stmts).all(|stmt| {
        !matches!(
            stmt,
            crate::Statement::Assign(Place::Local(other), _)
                if *other == local
        ) || matches!(
            stmt,
            crate::Statement::Assign(
                Place::Local(other),
                crate::Rvalue::Use(Operand::Copy(Place::Index { .. }))
            ) if *other == local
        )
    })
}
