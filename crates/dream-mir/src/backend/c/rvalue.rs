use crate::backend::c::calls::{emit_call, emit_iface, emit_indirect, emit_js_call};
use crate::backend::c::ctx::Cx;
use crate::backend::c::places::emit_operand;
use crate::backend::c::types::{c_ident, elem_size, load_cast, runtime_c_name};
use crate::{Rvalue, UnOp};
use dream_types::{PrimTy, TyKind};

pub(super) fn emit_rvalue(
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    rv: &Rvalue,
) -> String {
    match rv {
        Rvalue::Use(o) => emit_operand(cx, f, o),
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => format!(
            "({}) ? ({}) : ({})",
            emit_operand(cx, f, cond),
            emit_operand(cx, f, then_val),
            emit_operand(cx, f, else_val)
        ),
        Rvalue::Binary(op, a, b) => {
            if *op == crate::BinOp::Eq
                && matches!(
                    operand_kind(cx, f, a),
                    Some(TyKind::Prim(PrimTy::String))
                )
            {
                return format!(
                    "dream_string_eq({}, {})",
                    emit_operand(cx, f, a),
                    emit_operand(cx, f, b)
                );
            }
            if matches!(op, crate::BinOp::Div | crate::BinOp::Rem)
                && is_integer_operand(cx, f, a)
            {
                let lhs = emit_operand(cx, f, a);
                let rhs = emit_operand(cx, f, b);
                let symbol = cx.str_sym("panic: attempt to divide by zero");
                return format!(
                    "({{ int64_t __rhs = (int64_t)({rhs}); __rhs == 0 ? (dream_panic({symbol}), 0) : (({lhs}) {} __rhs); }})",
                    crate::backend::c::types::bin(*op)
                );
            }
            format!(
                "({}) {} ({})",
                emit_operand(cx, f, a),
                crate::backend::c::types::bin(*op),
                emit_operand(cx, f, b)
            )
        }
        Rvalue::Unary(UnOp::Neg, a) => format!("-({})", emit_operand(cx, f, a)),
        Rvalue::Unary(UnOp::Not, a) => format!("!({})", emit_operand(cx, f, a)),
        Rvalue::Unary(UnOp::BitNot, a) => format!("~({})", emit_operand(cx, f, a)),
        Rvalue::StrLen(s) => format!("dream_str_len({})", emit_operand(cx, f, s)),
        Rvalue::StrByteSize(s) => format!("dream_str_byte_size({})", emit_operand(cx, f, s)),
        Rvalue::CharAt(s, i, _) => format!(
            "((int32_t)dream_char_at_u({}, (int32_t)({})))",
            emit_operand(cx, f, s),
            emit_operand(cx, f, i)
        ),
        Rvalue::ByteAt(s, i, _) => format!(
            "((int32_t)dream_byte_at_u({}, (int32_t)({})))",
            emit_operand(cx, f, s),
            emit_operand(cx, f, i)
        ),
        Rvalue::ArrayNew { elem_ty, len } => {
            let es = elem_size(cx, *elem_ty);
            let n = emit_operand(cx, f, len);
            format!("dream_array_new({n}, {es})")
        }
        Rvalue::HashCode(o) => format!("dream_hash_value({})", emit_operand(cx, f, o)),
        Rvalue::ToString(o) => {
            let ty = operand_ty(cx, f, o);
            let conv = to_string_fn(cx, ty);
            if conv.is_empty() {
                emit_operand(cx, f, o)
            } else {
                format!("{conv}({})", emit_operand(cx, f, o))
            }
        }
        Rvalue::Concat(parts) => {
            if parts.is_empty() {
                cx.str_sym("").to_string()
            } else {
                let mut e = emit_operand(cx, f, &parts[0]);
                for p in &parts[1..] {
                    e = format!("dream_concat_strings({e}, {})", emit_operand(cx, f, p));
                }
                e
            }
        }
        Rvalue::ConcatInt {
            prefix,
            value,
            suffix,
        } => format!(
            "dream_concat_strings(dream_concat_strings({}, dream_int_to_string((int32_t)({}))), {})",
            emit_operand(cx, f, prefix),
            emit_operand(cx, f, value),
            emit_operand(cx, f, suffix)
        ),
        Rvalue::EnumName { value, arms } => {
            let v = emit_operand(cx, f, value);
            let mut e = cx.str_sym("").to_string();
            for (k, name) in arms.iter().rev() {
                e = format!(
                    "((int64_t)({v}) == {k}LL) ? ({}) : ({e})",
                    cx.str_sym(name)
                );
            }
            e
        }
        Rvalue::Call { callee, args } => emit_call(cx, f, callee, args),
        Rvalue::IndirectCall { target, args, sig } => {
            let call = emit_indirect(cx, f, target, args);
            match cx.interner.kind(*sig) {
                TyKind::Func(_, ret)
                    if matches!(
                        cx.interner.kind(*ret),
                        TyKind::Prim(
                            PrimTy::Int
                                | PrimTy::UInt
                                | PrimTy::Bool
                                | PrimTy::Byte
                                | PrimTy::Char
                        ) | TyKind::Enum(_)
                    ) =>
                {
                    format!("(int32_t)(intptr_t)({call})")
                }
                _ => call,
            }
        }
        Rvalue::InterfaceCall {
            receiver,
            iface_id,
            method_slot,
            args,
            ..
        } => emit_iface(cx, f, receiver, *iface_id, *method_slot, args),
        Rvalue::FuncRef(callee) => {
            let idx = cx
                .ft
                .get(&(callee.def, callee.args.clone()))
                .copied()
                .unwrap_or(0);
            format!("{idx}")
        }
        Rvalue::New { def, ty, ctor, args } => emit_new(cx, f, *def, *ty, *ctor, args),
        Rvalue::Tuple { ty, elems } => emit_tuple(cx, f, *ty, elems),
        Rvalue::UnionNew {
            def,
            ty,
            variant,
            args,
        } => emit_union_new(cx, f, *def, *ty, *variant, args),
        Rvalue::ArrayLit { elem_ty, elems } => emit_array_lit(cx, f, *elem_ty, elems),
        Rvalue::ArrayLen(a) => format!("*(int32_t*)dream_p({})", emit_operand(cx, f, a)),
        Rvalue::ToBytes { value, ty } => {
            let sz = elem_size(cx, *ty);
            let value = emit_operand(cx, f, value);
            match cx.interner.kind(*ty) {
                TyKind::Prim(PrimTy::String) => format!("dream_to_bytes({value}, {sz})"),
                TyKind::Prim(_) | TyKind::Enum(_) => {
                    let cast = load_cast(cx, *ty);
                    format!(
                        "dream_to_bytes((dream_ptr)(uintptr_t)&({cast}){{({cast})({value})}}, {sz})"
                    )
                }
                _ => format!("dream_to_bytes({value}, {sz})"),
            }
        }
        Rvalue::FromBytes { bytes, ty } => {
            let tag = cx.type_tag(*ty, dream_types::DefId(0));
            let sz = elem_size(cx, *ty);
            let bytes = emit_operand(cx, f, bytes);
            let from_bytes = format!("dream_from_bytes({bytes}, {sz}, {tag})");
            match cx.interner.kind(*ty) {
                TyKind::Prim(PrimTy::String) => from_bytes,
                TyKind::Prim(_) | TyKind::Enum(_) => {
                    let cast = load_cast(cx, *ty);
                    format!("(*({cast}*)dream_p({from_bytes}))")
                }
                _ => from_bytes,
            }
        }
        Rvalue::ArrayRealloc {
            elem_ty,
            array,
            new_len,
        } => {
            let es = elem_size(cx, *elem_ty);
            format!(
                "dream_array_realloc({}, {}, {es})",
                emit_operand(cx, f, array),
                emit_operand(cx, f, new_len)
            )
        }
        Rvalue::Cast(v, from, to) => emit_cast(cx, f, v, *from, *to),
        Rvalue::Discriminant(o) => {
            format!("*(int32_t*)dream_p({})", emit_operand(cx, f, o))
        }
        Rvalue::UnionField {
            base,
            ty,
            variant,
            field,
        } => {
            let u = cx.nunion(*ty).unwrap_or_else(|| {
                crate::internal_error!("missing union layout for {ty:?}")
            });
            let var = u
                .variants
                .iter()
                .find(|v| v.discriminant as usize == *variant)
                .unwrap_or_else(|| crate::internal_error!("missing union variant {variant}"));
            let fld = var.fields.get(*field).unwrap_or_else(|| {
                crate::internal_error!("missing union field {field}")
            });
            if cx.interner.is_value_type(fld.ty) {
                return format!(
                    "((dream_ptr)((char*)dream_p({}) + {}))",
                    emit_operand(cx, f, base),
                    fld.offset
                );
            }
            let cast = load_cast(cx, fld.ty);
            format!(
                "(*({cast}*)((char*)dream_p({}) + {}))",
                emit_operand(cx, f, base),
                fld.offset
            )
        }
        Rvalue::IsType(o, ty) => {
            let tag = runtime_tag(cx, *ty);
            format!(
                "(dream_object_tag({}) == {tag})",
                emit_operand(cx, f, o)
            )
        }
        Rvalue::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => emit_js_call(cx, f, target, via, method, args),
    }
}

fn operand_ty(
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    o: &crate::Operand,
) -> dream_types::TypeId {
    match o {
        crate::Operand::Copy(crate::Place::Local(l)) => f.local_ty(*l),
        crate::Operand::Copy(crate::Place::Global(g)) => cx
            .mir
            .globals
            .iter()
            .find(|x| x.id == *g)
            .map(|x| x.ty)
            .unwrap_or_else(|| cx.interner.int()),
        crate::Operand::Const(c) => match c {
            crate::Const::Int(_) => cx.interner.int(),
            crate::Const::Long(_) => cx.interner.long(),
            crate::Const::Float(_) => cx.interner.double(),
            crate::Const::F32(_) => cx.interner.float(),
            crate::Const::Bool(_) => cx.interner.bool(),
            crate::Const::Char(_) => cx.interner.char(),
            crate::Const::Str(_) => cx.interner.string(),
            crate::Const::Null => cx.interner.int(),
        },
        crate::Operand::Copy(_) => cx.interner.int(),
    }
}

fn operand_kind<'a>(
    cx: &'a Cx<'_>,
    f: &crate::MirFunction,
    o: &crate::Operand,
) -> Option<&'a TyKind> {
    match o {
        crate::Operand::Copy(crate::Place::Local(l)) => Some(cx.interner.kind(f.local_ty(*l))),
        crate::Operand::Const(crate::Const::Str(_)) => Some(cx.interner.kind(cx.interner.string())),
        _ => None,
    }
}

fn is_integer_operand(cx: &Cx<'_>, f: &crate::MirFunction, o: &crate::Operand) -> bool {
    if matches!(
        o,
        crate::Operand::Const(crate::Const::Int(_) | crate::Const::Long(_))
    ) {
        return true;
    }
    matches!(
        operand_kind(cx, f, o),
        Some(TyKind::Prim(
            PrimTy::Int | PrimTy::UInt | PrimTy::Long | PrimTy::ULong | PrimTy::Byte
        )) | Some(TyKind::Enum(_))
    )
}

fn runtime_tag(cx: &Cx<'_>, ty: dream_types::TypeId) -> i32 {
    match cx.interner.kind(ty) {
        TyKind::Prim(PrimTy::Int) => crate::abi::TAG_INT,
        TyKind::Prim(PrimTy::Float) => crate::abi::TAG_FLOAT,
        TyKind::Prim(PrimTy::Double) => crate::abi::TAG_DOUBLE,
        TyKind::Prim(PrimTy::Bool) => crate::abi::TAG_BOOL,
        TyKind::Prim(PrimTy::String) => crate::abi::TAG_STRING,
        TyKind::Prim(PrimTy::Char) => crate::abi::TAG_CHAR,
        TyKind::Prim(PrimTy::Long) => crate::abi::TAG_LONG,
        TyKind::Prim(PrimTy::UInt) => crate::abi::TAG_UINT,
        TyKind::Prim(PrimTy::ULong) => crate::abi::TAG_ULONG,
        TyKind::Prim(PrimTy::Byte) => crate::abi::TAG_BYTE,
        TyKind::Array(_) => crate::abi::TAG_ARRAY,
        _ => cx.type_tag(ty, dream_types::DefId(0)),
    }
}

fn emit_new(
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    def: dream_types::DefId,
    ty: dream_types::TypeId,
    ctor: Option<dream_types::DefId>,
    args: &[crate::Operand],
) -> String {
    let layout = cx.nstruct(ty).unwrap_or_else(|| {
        crate::internal_error!("missing layout for struct allocation {ty:?}")
    });
    let mut size = layout.size;
    if cx.interner.is_shared_type(ty) {
        size += 4;
    }
    let tag = cx.type_tag(ty, def);
    let mut s = format!("({{ dream_ptr __o = dream_malloc({size}, {tag}); memset(dream_p(__o), 0, {size}); ");
    if let Some(ctor) = ctor {
        let name = runtime_c_name(&cx.callee_c(ctor, &[]));
        let mut call_args = vec!["__o".to_string()];
        call_args.extend(args.iter().map(|a| emit_operand(cx, f, a)));
        s.push_str(&format!("{name}({}); ", call_args.join(", ")));
    }
    s.push_str("__o; })");
    s
}

fn emit_union_new(
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    def: dream_types::DefId,
    ty: dream_types::TypeId,
    variant: usize,
    args: &[crate::Operand],
) -> String {
    let u = cx.nunion(ty).unwrap_or_else(|| {
        crate::internal_error!("missing union layout {ty:?}")
    });
    let var = u
        .variants
        .iter()
        .find(|v| v.discriminant as usize == variant)
        .unwrap_or_else(|| crate::internal_error!("missing variant {variant}"));
    let tag = cx.type_tag(ty, def);
    let size = u.size;
    let mut s = format!(
        "({{ dream_ptr __o = dream_malloc({size}, {tag}); memset(dream_p(__o), 0, {size}); *(int32_t*)dream_p(__o) = {variant}; "
    );
    for (i, arg) in args.iter().enumerate() {
        let fld = &var.fields[i];
        if cx.interner.is_value_type(fld.ty) {
            let size = elem_size(cx, fld.ty);
            s.push_str(&format!(
                "memcpy((char*)dream_p(__o) + {}, dream_p({}), {size}); ",
                fld.offset,
                emit_operand(cx, f, arg)
            ));
            continue;
        }
        let cast = load_cast(cx, fld.ty);
        s.push_str(&format!(
            "*({cast}*)((char*)dream_p(__o) + {}) = ({cast})({}); ",
            fld.offset,
            emit_operand(cx, f, arg)
        ));
        if cx.interner.is_rc_tracked(fld.ty) {
            s.push_str(&format!(
                "dream_retain(*(dream_ptr *)((char*)dream_p(__o) + {})); ",
                fld.offset
            ));
        }
    }
    s.push_str("__o; })");
    s
}

fn emit_array_lit(
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    elem_ty: dream_types::TypeId,
    elems: &[crate::Operand],
) -> String {
    let es = elem_size(cx, elem_ty);
    let n = elems.len();
    let size = 4 + es * n as u32;
    let cast = load_cast(cx, elem_ty);
    let mut s = format!(
        "({{ dream_ptr __o = dream_malloc({size}, {}); memset(dream_p(__o), 0, {size}); *(int32_t*)dream_p(__o) = {n}; ",
        crate::abi::TAG_ARRAY
    );
    for (i, e) in elems.iter().enumerate() {
        let value = emit_operand(cx, f, e);
        if cx.interner.is_value_type(elem_ty) {
            s.push_str(&format!(
                "memcpy((char*)dream_p(__o) + 4 + {i}*{es}, dream_p({value}), {es}); "
            ));
        } else {
            s.push_str(&format!(
                "*({cast}*)((char*)dream_p(__o) + 4 + {i}*{es}) = ({cast})({value}); "
            ));
        }
    }
    s.push_str("__o; })");
    s
}

fn emit_tuple(
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    ty: dream_types::TypeId,
    elems: &[crate::Operand],
) -> String {
    let layout = cx.nstruct(ty).unwrap_or_else(|| {
        crate::internal_error!("missing tuple layout {ty:?}")
    });
    let size = layout.size.max(1);
    let mut s = format!(
        "({{ dream_ptr __o = dream_malloc({size}, {}); memset(dream_p(__o), 0, {size}); ",
        cx.type_tag(ty, dream_types::DefId(0))
    );
    for (i, e) in elems.iter().enumerate() {
        if let Some(fld) = layout.fields.get(i) {
            if cx.interner.is_value_type(fld.ty) {
                let size = elem_size(cx, fld.ty);
                s.push_str(&format!(
                    "memcpy((char*)dream_p(__o) + {}, dream_p({}), {size}); ",
                    fld.offset,
                    emit_operand(cx, f, e)
                ));
                continue;
            }
            let cast = load_cast(cx, fld.ty);
            s.push_str(&format!(
                "*({cast}*)((char*)dream_p(__o) + {}) = ({cast})({}); ",
                fld.offset,
                emit_operand(cx, f, e)
            ));
        }
    }
    s.push_str("__o; })");
    s
}

fn emit_cast(
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    v: &crate::Operand,
    from: dream_types::TypeId,
    to: dream_types::TypeId,
) -> String {
    let src = emit_operand(cx, f, v);
    if from == to {
        return src;
    }
    let fk = cx.interner.kind(from);
    let tk = cx.interner.kind(to);
    match (fk, tk) {
        (TyKind::Prim(PrimTy::Int), TyKind::Prim(PrimTy::Long)) => {
            format!("((int64_t)(int32_t)({src}))")
        }
        (TyKind::Prim(PrimTy::Int), TyKind::Prim(PrimTy::Double)) => {
            format!("((double)(int32_t)({src}))")
        }
        (TyKind::Prim(PrimTy::Int), TyKind::Prim(PrimTy::Float)) => {
            format!("((float)(int32_t)({src}))")
        }
        (TyKind::Prim(PrimTy::Float), TyKind::Prim(PrimTy::Double)) => {
            format!("((double)(float)({src}))")
        }
        (TyKind::Prim(PrimTy::Double), TyKind::Prim(PrimTy::Float)) => {
            format!("((float)({src}))")
        }
        (TyKind::Prim(PrimTy::Double), TyKind::Prim(PrimTy::Int)) => {
            format!("((int32_t)({src}))")
        }
        (TyKind::Prim(PrimTy::Float), TyKind::Prim(PrimTy::Int)) => {
            format!("((int32_t)({src}))")
        }
        (TyKind::Prim(PrimTy::Long), TyKind::Prim(PrimTy::Int)) => {
            format!("((int32_t)({src}))")
        }
        (_, TyKind::Object) => match fk {
            TyKind::Prim(PrimTy::Int) => format!("dream_box_int((int32_t)({src}))"),
            TyKind::Prim(PrimTy::Float) => format!("dream_box_float((float)({src}))"),
            TyKind::Prim(PrimTy::Double) => format!("dream_box_double((double)({src}))"),
            TyKind::Prim(PrimTy::Bool) => format!("dream_box_bool((int32_t)({src}))"),
            TyKind::Prim(PrimTy::Char) => format!("dream_box_char((int32_t)({src}))"),
            TyKind::Prim(PrimTy::Long) => format!("dream_box_long((int64_t)({src}))"),
            TyKind::Prim(PrimTy::UInt) => format!("dream_box_uint((int32_t)({src}))"),
            TyKind::Prim(PrimTy::ULong) => format!("dream_box_ulong((int64_t)({src}))"),
            TyKind::Prim(PrimTy::Byte) => format!("dream_box_byte((int32_t)({src}))"),
            _ => src,
        },
        (TyKind::Object, TyKind::Prim(p)) => match p {
            PrimTy::Int => format!("dream_unbox_int({src})"),
            PrimTy::Float => format!("dream_unbox_float({src})"),
            PrimTy::Double => format!("dream_unbox_double({src})"),
            PrimTy::Bool => format!("dream_unbox_bool({src})"),
            PrimTy::Char => format!("dream_unbox_char({src})"),
            PrimTy::Long => format!("dream_unbox_long({src})"),
            PrimTy::UInt => format!("dream_unbox_uint({src})"),
            PrimTy::ULong => format!("dream_unbox_ulong({src})"),
            PrimTy::Byte => format!("dream_unbox_byte({src})"),
            PrimTy::String => src,
        },
        _ => format!("({})", src),
    }
}

pub(super) fn to_string_fn(cx: &Cx<'_>, ty: dream_types::TypeId) -> String {
    match cx.interner.kind(ty) {
        TyKind::Prim(PrimTy::Int) => "dream_int_to_string".into(),
        TyKind::Prim(PrimTy::UInt) => "dream_uint_to_string".into(),
        TyKind::Prim(PrimTy::Long) => "dream_long_to_string".into(),
        TyKind::Prim(PrimTy::ULong) => "dream_ulong_to_string".into(),
        TyKind::Prim(PrimTy::Byte) => "dream_byte_to_string".into(),
        TyKind::Prim(PrimTy::Bool) => "dream_bool_to_string".into(),
        TyKind::Prim(PrimTy::Char) => "dream_char_to_string".into(),
        TyKind::Prim(PrimTy::Float) => "dream_float_to_string".into(),
        TyKind::Prim(PrimTy::Double) => "dream_double_to_string".into(),
        TyKind::Prim(PrimTy::String) => {
            // identity; caller should retain if needed
            String::new()
        }
        TyKind::Enum(_) => "dream_int_to_string".into(),
        TyKind::Array(e) => c_ident(&format!("array_to_string_t{}", e.0)),
        _ => {
            if let Some(l) = cx.nstruct(ty) {
                c_ident(&format!("{}_to_string", l.name))
            } else if let Some(u) = cx.nunion(ty) {
                c_ident(&format!("{}_to_string", u.name))
            } else {
                "dream_object_to_string".into()
            }
        }
    }
}
