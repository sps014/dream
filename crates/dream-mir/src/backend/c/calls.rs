use crate::backend::c::ctx::Cx;
use crate::backend::c::places::emit_operand;
use crate::backend::c::types::{c_ident, runtime_c_name};
use crate::{Callee, Operand};

pub(super) fn emit_call(
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    callee: &Callee,
    args: &[Operand],
) -> String {
    let raw = cx.callee_c(callee.def, &callee.args);
    let name = runtime_c_name(&raw);
    let mut args_s: Vec<String> = args.iter().map(|a| emit_operand(cx, f, a)).collect();
    if name == "dream_all" {
        let es = match cx.interner.kind(callee.ret) {
            dream_types::TyKind::Array(e) => crate::backend::c::types::elem_size(cx, *e),
            _ => callee
                .args
                .first()
                .map(|t| crate::backend::c::types::elem_size(cx, *t))
                .unwrap_or(4),
        };
        args_s.push(es.to_string());
    }
    format!("{name}({})", args_s.join(", "))
}

pub(super) fn emit_indirect(
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    target: &Operand,
    args: &[Operand],
) -> String {
    let mut args_s: Vec<String> = args.iter().map(|a| emit_operand(cx, f, a)).collect();
    while args_s.len() < 8 {
        args_s.push("0".into());
    }
    let joined = args_s.join(", ");
    format!(
        "((dream_fn)dream_ft[((uintptr_t)({}) < 65536u) ? (int32_t)(uintptr_t)({}) : dream_funcbox_funcidx((dream_ptr)({}))])({joined})",
        emit_operand(cx, f, target),
        emit_operand(cx, f, target),
        emit_operand(cx, f, target)
    )
}

pub(super) fn emit_iface(
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    receiver: &Operand,
    iface_id: usize,
    method_slot: usize,
    args: &[Operand],
) -> String {
    let mut all = vec![emit_operand(cx, f, receiver)];
    all.extend(args.iter().map(|a| emit_operand(cx, f, a)));
    while all.len() < 8 {
        all.push("0".into());
    }
    format!(
        "{}({})",
        c_ident(&format!("__iface_dispatch_{iface_id}_{method_slot}")),
        all.join(", ")
    )
}

pub(super) fn emit_js_call(
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    target: &Operand,
    via: &Option<Operand>,
    method: &Option<Operand>,
    args: &[(Operand, dream_types::TypeId)],
) -> String {
    let t = emit_operand(cx, f, target);
    let v = via
        .as_ref()
        .map(|o| emit_operand(cx, f, o))
        .unwrap_or_else(|| "0".into());
    let m = method
        .as_ref()
        .map(|o| emit_operand(cx, f, o))
        .unwrap_or_else(|| "0".into());
    let argc = args.len();
    format!("dream_js_call({t}, {v}, {m}, {argc})")
}
