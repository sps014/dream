use crate::backend::c::calls::{emit_call, emit_iface, emit_indirect, emit_js_call};
use crate::backend::c::ctx::Cx;
use crate::backend::c::places::{emit_operand, emit_store};
use crate::backend::c::rvalue::{emit_rvalue, to_string_fn};
use crate::backend::c::types::elem_size;
use crate::{Place, Rvalue, Statement};
use dream_types::{PrimTy, TyKind};

pub(super) fn emit_stmt(out: &mut String, cx: &Cx<'_>, f: &crate::MirFunction, stmt: &Statement) {
    match stmt {
        Statement::Nop | Statement::SourceLine(_) | Statement::DebugLine(_) => {}
        Statement::Assign(place, rv) => {
            let rhs = emit_rvalue(cx, f, rv);
            out.push_str("  ");
            out.push_str(&emit_store(cx, f, place, rv, &rhs));
            out.push_str(";\n");
            if field_store_copies_ref(cx, f, place, rv) {
                out.push_str(&format!("  dream_retain({rhs});\n"));
            }
        }
        Statement::Retain(o) => {
            out.push_str(&format!("  dream_retain({});\n", emit_operand(cx, f, o)));
        }
        Statement::Release(o) => {
            let ty = operand_ty(cx, f, o);
            let release = if cx.interner.is_rc_tracked(ty) {
                crate::backend::c::release::release_sym(cx.interner, cx.mir, ty)
            } else {
                "dream_release".into()
            };
            out.push_str(&format!("  {release}({});\n", emit_operand(cx, f, o)));
        }
        Statement::Panic(o) => {
            out.push_str(&format!("  dream_panic({});\n", emit_operand(cx, f, o)));
        }
        Statement::Print { arg, ty, newline } => {
            emit_print(out, cx, f, arg, *ty, *newline);
        }
        Statement::Call { callee, args } => {
            out.push_str(&format!("  {};\n", emit_call(cx, f, callee, args)));
        }
        Statement::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            out.push_str(&format!(
                "  (void){};\n",
                emit_js_call(cx, f, target, via, method, args)
            ));
        }
        Statement::InterfaceCall {
            receiver,
            iface_id,
            method_slot,
            args,
            ..
        } => {
            out.push_str(&format!(
                "  (void){};\n",
                emit_iface(cx, f, receiver, *iface_id, *method_slot, args)
            ));
        }
        Statement::IndirectCall { target, args, .. } => {
            out.push_str(&format!(
                "  (void){};\n",
                emit_indirect(cx, f, target, args)
            ));
        }
        Statement::ArrayElemsCopy {
            dst,
            dst_off,
            src,
            src_off,
            count,
            elem_ty,
        } => {
            let es = elem_size(cx, *elem_ty);
            out.push_str(&format!(
                "  dream_mem_copy({}+4+({})*{es}, {}+4+({})*{es}, (size_t)({})*{es});\n",
                emit_operand(cx, f, dst),
                emit_operand(cx, f, dst_off),
                emit_operand(cx, f, src),
                emit_operand(cx, f, src_off),
                emit_operand(cx, f, count),
            ));
        }
        Statement::ArrayElemsFill {
            dst,
            dst_off,
            count,
            elem_ty,
        } => {
            let es = elem_size(cx, *elem_ty);
            out.push_str(&format!(
                "  memset((char*)dream_p({}) + 4 + ({})*{es}, 0, (size_t)({})*{es});\n",
                emit_operand(cx, f, dst),
                emit_operand(cx, f, dst_off),
                emit_operand(cx, f, count),
            ));
        }
        Statement::ForceFree(o) => {
            out.push_str(&format!("  dream_free({});\n", emit_operand(cx, f, o)));
        }
        Statement::LockAcquire(o) => {
            out.push_str(&format!(
                "  dream_lock_acquire({});\n",
                emit_operand(cx, f, o)
            ));
        }
        Statement::LockRelease(o) => {
            out.push_str(&format!(
                "  dream_lock_release({});\n",
                emit_operand(cx, f, o)
            ));
        }
        Statement::SimdV128 {
            dest,
            lhs,
            rhs,
            index,
            splat_rhs,
            ptr_addr,
            op,
            lane,
        } => {
            let d = emit_operand(cx, f, dest);
            let l = emit_operand(cx, f, lhs);
            let r = emit_operand(cx, f, rhs);
            let i = emit_operand(cx, f, index);
            let es = lane.esize();
            let opi = match op {
                crate::BinOp::Add => 0,
                crate::BinOp::Sub => 1,
                crate::BinOp::Mul => 2,
                crate::BinOp::Div => 3,
                _ => 0,
            };
            let raddr = splat_rhs
                .as_ref()
                .map(|s| emit_operand(cx, f, s))
                .unwrap_or_else(|| r.clone());
            let call = if *ptr_addr {
                format!("dream_simd_binop({d}, {l}, {raddr}, {es}, {opi})")
            } else {
                format!(
                    "dream_simd_binop({d}+4+({i})*{es}, {l}+4+({i})*{es}, {raddr}+4+({i})*{es}, {es}, {opi})"
                )
            };
            out.push_str(&format!("  {call};\n"));
        }
        Statement::ValueDrop(l) => {
            emit_value_refs(out, cx, f.local_ty(*l), &format!("l{}", l.0), false);
        }
        Statement::ValueRetain(l) => {
            emit_value_refs(out, cx, f.local_ty(*l), &format!("l{}", l.0), true);
        }
        Statement::ValueKill(l) => {
            let size = elem_size(cx, f.local_ty(*l));
            out.push_str(&format!("  memset(dream_p(l{}), 0, {size});\n", l.0));
        }
    }
}

pub(super) fn emit_value_refs(
    out: &mut String,
    cx: &Cx<'_>,
    ty: dream_types::TypeId,
    base: &str,
    retain: bool,
) {
    let Some(layout) = cx.nstruct(ty) else {
        return;
    };
    for field in &layout.fields {
        if field.is_weak || field.is_unowned {
            continue;
        }
        let at = format!("((dream_ptr)((char *)dream_p({base}) + {}))", field.offset);
        if cx.interner.is_value_type(field.ty) {
            emit_value_refs(out, cx, field.ty, &at, retain);
        } else if cx.interner.is_rc_tracked(field.ty) {
            let value = format!("*(dream_ptr *)dream_p({at})");
            if retain {
                out.push_str(&format!("  dream_retain({value});\n"));
            } else {
                let release = crate::backend::c::release::release_sym(cx.interner, cx.mir, field.ty);
                out.push_str(&format!("  {release}({value});\n"));
            }
        }
    }
}

fn operand_ty(
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    o: &crate::Operand,
) -> dream_types::TypeId {
    match o {
        crate::Operand::Copy(Place::Local(l)) => f.local_ty(*l),
        crate::Operand::Copy(Place::Global(g)) => cx
            .mir
            .globals
            .iter()
            .find(|global| global.id == *g)
            .map(|global| global.ty)
            .unwrap_or_else(|| cx.interner.int()),
        crate::Operand::Copy(Place::Field { base, field }) => cx
            .nstruct(f.local_ty(*base))
            .and_then(|layout| layout.fields.get(*field))
            .map(|field| field.ty)
            .unwrap_or_else(|| f.local_ty(*base)),
        crate::Operand::Copy(Place::Index { base, .. }) => {
            crate::backend::c::types::array_elem_ty(cx.interner, f.local_ty(*base))
        }
        crate::Operand::Copy(Place::Deref { elem_ty, .. }) => *elem_ty,
        crate::Operand::Const(crate::Const::Str(_)) => cx.interner.string(),
        crate::Operand::Const(crate::Const::Long(_)) => cx.interner.long(),
        crate::Operand::Const(crate::Const::Float(_)) => cx.interner.double(),
        crate::Operand::Const(crate::Const::F32(_)) => cx.interner.float(),
        crate::Operand::Const(crate::Const::Bool(_)) => cx.interner.bool(),
        crate::Operand::Const(crate::Const::Char(_)) => cx.interner.char(),
        crate::Operand::Const(_) => cx.interner.int(),
    }
}

fn field_store_copies_ref(cx: &Cx<'_>, f: &crate::MirFunction, place: &Place, rv: &Rvalue) -> bool {
    let Place::Field { base, field } = place else {
        return false;
    };
    let Some(layout) = cx.nstruct(f.local_ty(*base)) else {
        return false;
    };
    let Some(fld) = layout.fields.get(*field) else {
        crate::internal_error!("missing field {field} on type {:?}", f.local_ty(*base));
    };
    !fld.is_weak
        && !fld.is_unowned
        && cx.interner.is_rc_tracked(fld.ty)
        && matches!(
            rv,
            Rvalue::Use(crate::Operand::Copy(_))
                | Rvalue::Use(crate::Operand::Const(crate::Const::Str(_)))
        )
}

fn emit_print(
    out: &mut String,
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    arg: &crate::Operand,
    ty: dream_types::TypeId,
    newline: bool,
) {
    let a = emit_operand(cx, f, arg);
    match cx.interner.kind(ty) {
        TyKind::Prim(PrimTy::Int) | TyKind::Enum(_) => {
            out.push_str(&format!("  print_int((int32_t)({a}));\n"));
        }
        TyKind::Prim(PrimTy::Char) => {
            out.push_str(&format!("  print_char((int32_t)({a}));\n"));
        }
        TyKind::Prim(PrimTy::String) => {
            out.push_str(&format!("  print_string({a});\n"));
        }
        TyKind::Prim(PrimTy::Float) => {
            out.push_str(&format!("  print_float((float)({a}));\n"));
        }
        TyKind::Prim(PrimTy::Double) => {
            out.push_str(&format!("  print_double((double)({a}));\n"));
        }
        _ => {
            let conv = to_string_fn(cx, ty);
            if conv.is_empty() {
                out.push_str(&format!("  print_string({a});\n"));
            } else {
                out.push_str(&format!("  print_string({conv}({a}));\n"));
            }
        }
    }
    if newline {
        out.push_str("  print_char(10);\n");
    }
}
