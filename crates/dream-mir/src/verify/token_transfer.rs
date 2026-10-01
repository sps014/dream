//! Local copies still spell a move as copy followed by source nulling at the insertion boundary.
//! Container transfers already carry `Move`; calls carry parameter ownership in their ABI.

use super::token_flow::{Flow, State, Ticket};
use crate::{Const, Local, Operand, Place, Rvalue, Statement};
use std::collections::BTreeSet;

pub(super) fn call(
    flow: &mut Flow<'_>,
    state: &mut State,
    bi: usize,
    si: usize,
    takes: &[bool],
    args: &[Operand],
) {
    for (index, arg) in args.iter().enumerate() {
        if takes.get(index).copied().unwrap_or(false) {
            if let Operand::Copy(Place::Local(l)) = arg {
                flow.consume(state, *l, bi, si);
            } else if !matches!(arg, Operand::Const(_)) {
                flow.findings.insert((
                    bi,
                    si,
                    "taken argument was not normalized to an explicit local token".into(),
                ));
            }
        }
    }
}

pub(super) fn statement(
    flow: &mut Flow<'_>,
    state: &mut State,
    bi: usize,
    si: usize,
    stmt: &Statement,
) {
    let discarded = match stmt {
        Statement::Call { callee, .. } | Statement::JsCall { callee, .. } => Some(callee.ret),
        Statement::IndirectCall { sig, .. } | Statement::InterfaceCall { sig, .. } => {
            match flow.interner.kind(*sig) {
                dream_types::TyKind::Func(_, ret) => Some(*ret),
                _ => None,
            }
        }
        _ => None,
    };
    if discarded
        .is_some_and(|ty| flow.interner.is_rc_tracked(ty) || flow.interner.is_value_type(ty))
    {
        flow.findings.insert((
            bi,
            si,
            "owning call result lacks an explicit destination".into(),
        ));
    }
    match stmt {
        Statement::Call { callee, args } | Statement::Assign(_, Rvalue::Call { callee, args }) => {
            call(flow, state, bi, si, &callee.take_params, args)
        }
        Statement::Assign(
            _,
            Rvalue::New {
                ctor: Some(ctor),
                args,
                ..
            },
        ) => call(flow, state, bi, si, &ctor.take_params, args),
        _ => {}
    }
    match stmt {
        Statement::Retain(Operand::Copy(Place::Local(l))) => flow.retain(state, *l, bi, si),
        Statement::Release(Operand::Copy(Place::Local(l))) => flow.consume(state, *l, bi, si),
        Statement::Assign(Place::Local(dest), rv) if flow.tracked(*dest) => {
            if matches!(rv, Rvalue::Use(Operand::Copy(Place::Local(src))) if src == dest) {
                return;
            }
            flow.overwrite(state, *dest, bi, si);
            let source = match rv {
                Rvalue::Use(Operand::Copy(Place::Local(src))) => Some(*src),
                _ => None,
            };
            let moved = source.filter(|src| local_move(flow, bi, si, *src, *dest));
            let value = if let Some(src) = moved {
                let value = flow.operand(&Operand::Copy(Place::Local(src)), state);
                flow.consume(state, src, bi, si);
                value
                    .into_iter()
                    .map(|t| match t {
                        Ticket::Owned(_) => Ticket::Owned(1),
                        other => other,
                    })
                    .collect()
            } else {
                match rv {
                    Rvalue::Use(op) | Rvalue::Cast(op, _, _)
                        if crate::passes::rc::is_borrowed_copy(rv, flow.interner) =>
                    {
                        flow.operand(op, state)
                            .into_iter()
                            .map(|t| match t {
                                Ticket::Owned(_) => Ticket::Borrow,
                                other => other,
                            })
                            .collect()
                    }
                    Rvalue::Use(Operand::Const(Const::Null)) => BTreeSet::from([Ticket::Null]),
                    Rvalue::Move { src, .. } => {
                        let value = flow.operand(&Operand::Copy(Place::Local(*src)), state);
                        flow.consume(state, *src, bi, si);
                        flow.overwrite(state, *src, bi, si);
                        state[src.0 as usize] = BTreeSet::from([Ticket::Null]);
                        value
                            .into_iter()
                            .map(|t| match t {
                                Ticket::Owned(_) => Ticket::Owned(1),
                                other => other,
                            })
                            .collect()
                    }
                    Rvalue::UnionField { .. } => BTreeSet::from([Ticket::Borrow, Ticket::Null]),
                    _ => BTreeSet::from([Ticket::Owned(1), Ticket::Null]),
                }
            };
            state[dest.0 as usize] = value;
        }
        Statement::Assign(_, Rvalue::Move { src, .. }) => {
            flow.consume(state, *src, bi, si);
            flow.overwrite(state, *src, bi, si);
            state[src.0 as usize] = BTreeSet::from([Ticket::Null]);
        }
        _ => {}
    }
}

fn local_move(flow: &Flow<'_>, bi: usize, si: usize, src: Local, dest: Local) -> bool {
    for next in &flow.f.blocks[bi].stmts[si + 1..] {
        match next {
            Statement::Assign(Place::Local(l), Rvalue::Use(Operand::Const(Const::Null)))
                if *l == src =>
            {
                return true
            }
            Statement::Assign(Place::Local(l), Rvalue::Use(Operand::Const(Const::Null)))
                if *l != dest => {}
            Statement::Nop | Statement::DebugLine(_) | Statement::SourceLine(_) => {}
            _ => return false,
        }
    }
    false
}
