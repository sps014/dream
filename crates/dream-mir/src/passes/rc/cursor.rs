//! Cursor inference: mark non-escaping field/index loads as non-owning aliases.

use crate::{Callee, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;
use std::collections::HashSet;

/// Mark locals that only hold a non-escaping field/index (or union-field) load, or a forwarding
/// copy of another RC local, as cursors so [`super::RcInsertion`] skips retain/release on them.
pub(crate) fn infer_cursors(func: &mut MirFunction, interner: &TypeInterner) {
    let n = func.locals.len();
    let params: HashSet<u32> = func.params.iter().map(|p| p.0).collect();
    let mut candidates: HashSet<u32> = HashSet::new();
    let mut forwarding: HashSet<u32> = HashSet::new();
    let mut escaped: HashSet<u32> = HashSet::new();
    let mut def_count: Vec<u32> = vec![0; n];

    for block in &func.blocks {
        for stmt in &block.stmts {
            if let Statement::Assign(Place::Local(dest), rvalue) = stmt {
                let d = dest.0 as usize;
                if d < n {
                    def_count[d] += 1;
                }
                let rc = !params.contains(&dest.0)
                    && interner.is_rc_tracked(func.locals[dest.0 as usize].ty);
                if rc && is_cursor_source(rvalue) {
                    candidates.insert(dest.0);
                } else if rc && is_forwarding_copy(rvalue) {
                    if let Rvalue::Use(Operand::Copy(Place::Local(src))) = rvalue {
                        if func.locals[src.0 as usize].ty == func.locals[dest.0 as usize].ty {
                            forwarding.insert(dest.0);
                        } else {
                            escaped.insert(dest.0);
                            forwarding.remove(&dest.0);
                        }
                    }
                } else if !is_cursor_source(rvalue) && !is_forwarding_copy(rvalue) {
                    escaped.insert(dest.0);
                    forwarding.remove(&dest.0);
                }
            }
            mark_stmt_escapes(stmt, &mut escaped);
        }
        mark_term_escapes(&block.terminator, &mut escaped);
    }

    for block in &func.blocks {
        for stmt in &block.stmts {
            if let Statement::Assign(
                Place::Local(dest),
                Rvalue::Use(Operand::Copy(Place::Local(src))),
            ) = stmt
            {
                if !forwarding.contains(&dest.0) {
                    escaped.insert(src.0);
                }
            }
        }
    }

    for (i, &defs) in def_count.iter().enumerate() {
        let id = i as u32;
        if defs != 1 || params.contains(&id) || func.locals[i].is_take {
            escaped.insert(id);
        }
    }

    for id in candidates.union(&forwarding).copied() {
        if !escaped.contains(&id) {
            func.locals[id as usize].is_cursor = true;
        }
    }
}

fn is_forwarding_copy(rvalue: &Rvalue) -> bool {
    matches!(rvalue, Rvalue::Use(Operand::Copy(Place::Local(_))))
}

fn is_cursor_source(rvalue: &Rvalue) -> bool {
    matches!(
        rvalue,
        Rvalue::Use(Operand::Copy(Place::Field { .. }))
            | Rvalue::Use(Operand::Copy(Place::Index { .. }))
            | Rvalue::UnionField { .. }
            | Rvalue::Cast(Operand::Copy(Place::Field { .. }), _, _)
            | Rvalue::Cast(Operand::Copy(Place::Index { .. }), _, _)
    )
}

fn mark_stmt_escapes(stmt: &Statement, escaped: &mut HashSet<u32>) {
    match stmt {
        Statement::Assign(Place::Field { .. }, rvalue)
        | Statement::Assign(Place::Index { .. }, rvalue)
        | Statement::Assign(Place::Global(_), rvalue) => {
            escape_rvalue_payload(rvalue, escaped);
        }
        Statement::Assign(Place::Local(_), rvalue) => {
            escape_constructor_payloads(rvalue, escaped);
        }
        Statement::Call { callee, args } => escape_take_args(callee, args, escaped),
        Statement::IndirectCall { args, .. } | Statement::InterfaceCall { args, .. } => {
            for a in args {
                escape_operand(a, escaped);
            }
        }
        Statement::JsCall { args, .. } => {
            for (a, _) in args {
                escape_operand(a, escaped);
            }
        }
        Statement::ForceFree(op) => escape_operand(op, escaped),
        Statement::ArrayElemsCopy { dst, src, .. } => {
            escape_operand(dst, escaped);
            escape_operand(src, escaped);
        }
        Statement::ArrayElemsFill { dst, .. } => escape_operand(dst, escaped),
        _ => {}
    }
}

fn mark_term_escapes(term: &Terminator, escaped: &mut HashSet<u32>) {
    match term {
        Terminator::Return(Some(op)) | Terminator::AsyncComplete(Some(op)) => {
            escape_operand(op, escaped);
        }
        Terminator::TailCall { callee, args } => escape_take_args(callee, args, escaped),
        Terminator::Await { future, .. } => escape_operand(future, escaped),
        _ => {}
    }
}

fn escape_take_args(callee: &Callee, args: &[Operand], escaped: &mut HashSet<u32>) {
    for (i, arg) in args.iter().enumerate() {
        if callee.take_params.get(i).copied().unwrap_or(false) {
            escape_operand(arg, escaped);
        }
    }
}

fn escape_constructor_payloads(rvalue: &Rvalue, escaped: &mut HashSet<u32>) {
    match rvalue {
        Rvalue::Call { callee, args, .. } => escape_take_args(callee, args, escaped),
        Rvalue::New { args, .. }
        | Rvalue::UnionNew { args, .. }
        | Rvalue::ArrayLit { elems: args, .. }
        | Rvalue::Tuple { elems: args, .. } => {
            for a in args {
                escape_operand(a, escaped);
            }
        }
        Rvalue::IndirectCall { args, .. } | Rvalue::InterfaceCall { args, .. } => {
            for a in args {
                escape_operand(a, escaped);
            }
        }
        Rvalue::JsCall { args, .. } => {
            for (a, _) in args {
                escape_operand(a, escaped);
            }
        }
        _ => {}
    }
}

fn escape_rvalue_payload(rvalue: &Rvalue, escaped: &mut HashSet<u32>) {
    match rvalue {
        Rvalue::Use(op) | Rvalue::Cast(op, _, _) => escape_operand(op, escaped),
        other => escape_constructor_payloads(other, escaped),
    }
}

fn escape_operand(op: &Operand, escaped: &mut HashSet<u32>) {
    if let Operand::Copy(Place::Local(l)) = op {
        escaped.insert(l.0);
    }
}
