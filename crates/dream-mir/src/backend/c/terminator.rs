use crate::backend::c::calls::emit_call;
use crate::backend::c::ctx::Cx;
use crate::backend::c::places::emit_operand;
use crate::{BlockId, Operand, Terminator};
use dream_types::TyKind;

pub(super) fn emit_term(
    out: &mut String,
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    t: &Terminator,
) {
    match t {
        Terminator::Goto(b) => out.push_str(&format!("  goto L{};\n", b.0)),
        Terminator::If {
            cond,
            then_blk,
            else_blk,
        } => {
            out.push_str(&format!(
                "  if ({}) goto L{}; else goto L{};\n",
                emit_operand(cx, f, cond),
                then_blk.0,
                else_blk.0
            ));
        }
        Terminator::Switch {
            value,
            targets,
            default,
        } => emit_switch(out, cx, f, value, targets, *default),
        Terminator::Return(None) => {
            if f.is_async || !matches!(cx.interner.kind(f.ret), TyKind::Void) {
                out.push_str("  return 0;\n");
            } else {
                out.push_str("  return;\n");
            }
        }
        Terminator::Return(Some(o)) => {
            out.push_str(&format!("  return {};\n", emit_operand(cx, f, o)));
        }
        Terminator::Unreachable => out.push_str("  abort();\n"),
        Terminator::TailCall { callee, args } => {
            let call = emit_call(cx, f, callee, args);
            if matches!(cx.interner.kind(f.ret), TyKind::Void) {
                out.push_str(&format!("  {call}; return;\n"));
            } else {
                out.push_str(&format!("  return {call};\n"));
            }
        }
        Terminator::AsyncComplete(None) => {
            out.push_str("  dream_async_complete(__self, 0); return 0;\n");
        }
        Terminator::AsyncComplete(Some(o)) => {
            let result = emit_operand(cx, f, o);
            match cx.interner.kind(f.ret) {
                TyKind::Prim(dream_types::PrimTy::Long | dream_types::PrimTy::ULong) => {
                    out.push_str(&format!(
                        "  *(int64_t *)((char *)dream_p(__self) + 56) = (int64_t)({result});\n  dream_async_complete(__self, 0); return 0;\n"
                    ));
                }
                TyKind::Prim(dream_types::PrimTy::Float) => {
                    out.push_str(&format!(
                        "  *(float *)((char *)dream_p(__self) + 56) = (float)({result});\n  dream_async_complete(__self, 0); return 0;\n"
                    ));
                }
                TyKind::Prim(dream_types::PrimTy::Double) => {
                    out.push_str(&format!(
                        "  *(double *)((char *)dream_p(__self) + 56) = (double)({result});\n  dream_async_complete(__self, 0); return 0;\n"
                    ));
                }
                _ => out.push_str(&format!(
                    "  dream_async_complete(__self, (dream_ptr){result}); return 0;\n"
                )),
            }
        }
        Terminator::Await {
            future,
            dest: _,
            resume,
        } => {
            let fut = emit_operand(cx, f, future);
            out.push_str(&format!("  *(int32_t *)dream_p(__self) = {};\n", resume.0));
            out.push_str(&format!("  dream_await(__self, {fut});\n"));
            out.push_str("  return 0;\n");
        }
    }
}

fn emit_switch(
    out: &mut String,
    cx: &Cx<'_>,
    f: &crate::MirFunction,
    value: &Operand,
    targets: &[(i64, BlockId)],
    default: BlockId,
) {
    let v = emit_operand(cx, f, value);
    let mut keys: Vec<i64> = targets.iter().map(|(k, _)| *k).collect();
    keys.sort_unstable();
    let dense = !keys.is_empty()
        && keys[0] == 0
        && keys.windows(2).all(|w| w[1] == w[0] + 1)
        && keys.len() >= 2;
    if dense {
        out.push_str("  {\n");
        out.push_str("    static void *const __jt[] = {\n");
        let max = keys.last().copied().unwrap() as usize;
        let mut map = vec![default; max + 1];
        for (k, b) in targets {
            map[*k as usize] = *b;
        }
        for b in &map {
            out.push_str(&format!("      &&L{},\n", b.0));
        }
        out.push_str("    };\n");
        out.push_str(&format!(
            "    unsigned __k = (unsigned)({v}); if (__k < {}) goto *__jt[__k]; goto L{};\n",
            map.len(),
            default.0
        ));
        out.push_str("  }\n");
        return;
    }
    out.push_str(&format!("  switch ((int64_t)({v})) {{\n"));
    for (k, b) in targets {
        out.push_str(&format!("    case {k}: goto L{};\n", b.0));
    }
    out.push_str(&format!("    default: goto L{};\n", default.0));
    out.push_str("  }\n");
}
