//! Ownership obligations before optimizations erase local transfers. Pointer liveness is a
//! separate proof: consuming a local's token need not destroy an object shared by another owner.

use super::{violation, Violation};
use crate::{Const, Local, Mir, MirFunction, Operand, Place, Statement, Terminator};
use dream_types::TypeInterner;
use std::collections::{BTreeSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Ticket {
    Null,
    Borrow,
    Immortal,
    Owned(usize),
}

pub(super) type Slot = BTreeSet<Ticket>;
pub(super) type State = Vec<Slot>;

pub(super) fn assert_module(mir: &Mir, interner: &TypeInterner) {
    let found: Vec<_> = mir
        .functions
        .iter()
        .chain(&mir.polls)
        .flat_map(|f| {
            let mut out = Vec::new();
            if super::valid_cfg(f, &mut out) {
                out.extend(check(f, interner));
            }
            out
        })
        .collect();
    if !found.is_empty() {
        let lines: Vec<_> = found
            .iter()
            .map(|v| format!("{} bb{}[{}]: {}", v.func, v.block, v.stmt, v.msg))
            .collect();
        crate::internal_error!("RC token verifier failed:\n{}", lines.join("\n"));
    }
}

pub(super) struct Flow<'a> {
    pub f: &'a MirFunction,
    pub interner: &'a TypeInterner,
    pub limit: usize,
    pub findings: BTreeSet<(usize, usize, String)>,
}

impl Flow<'_> {
    pub fn error(&mut self, bi: usize, si: usize, local: Local, message: &str) {
        self.findings
            .insert((bi, si, format!("_{local}: {message}", local = local.0)));
    }

    pub fn tracked(&self, local: Local) -> bool {
        self.interner.is_rc_tracked(self.f.local_ty(local))
    }

    pub fn operand(&self, op: &Operand, state: &State) -> Slot {
        match op {
            Operand::Copy(Place::Local(l)) if self.tracked(*l) => state[l.0 as usize].clone(),
            Operand::Const(Const::Null) => BTreeSet::from([Ticket::Null]),
            Operand::Const(Const::Str(_)) => BTreeSet::from([Ticket::Immortal]),
            _ => BTreeSet::from([Ticket::Borrow, Ticket::Null]),
        }
    }

    pub fn consume(&mut self, state: &mut State, local: Local, bi: usize, si: usize) {
        if !self.tracked(local) {
            return;
        }
        let old = state[local.0 as usize].clone();
        state[local.0 as usize] = old
            .into_iter()
            .map(|ticket| match ticket {
                Ticket::Owned(1) => Ticket::Borrow,
                Ticket::Owned(n) => Ticket::Owned(n - 1),
                Ticket::Borrow => {
                    self.error(bi, si, local, "RC token consumed without ownership");
                    Ticket::Borrow
                }
                other => other,
            })
            .collect();
    }

    pub fn retain(&mut self, state: &mut State, local: Local, bi: usize, si: usize) {
        if !self.tracked(local) {
            return;
        }
        let old = state[local.0 as usize].clone();
        state[local.0 as usize] = old
            .into_iter()
            .map(|ticket| match ticket {
                Ticket::Borrow => Ticket::Owned(1),
                Ticket::Owned(n) if n < self.limit => Ticket::Owned(n + 1),
                Ticket::Owned(n) => {
                    self.error(bi, si, local, "RC tokens accumulate across a loop");
                    Ticket::Owned(n)
                }
                other => other,
            })
            .collect();
    }

    pub fn overwrite(&mut self, state: &State, local: Local, bi: usize, si: usize) {
        if state[local.0 as usize]
            .iter()
            .any(|t| matches!(t, Ticket::Owned(_)))
        {
            self.error(
                bi,
                si,
                local,
                "owned RC token overwritten without release or transfer",
            );
        }
    }

    pub fn exit(&mut self, state: &State, bi: usize, si: usize) {
        for (index, slot) in state.iter().enumerate() {
            if slot.iter().any(|t| matches!(t, Ticket::Owned(_))) {
                self.error(
                    bi,
                    si,
                    Local(index as u32),
                    "owned RC token remains at function exit",
                );
            }
        }
    }
}

pub(super) fn check(f: &MirFunction, interner: &TypeInterner) -> Vec<Violation> {
    let mut flow = Flow {
        f,
        interner,
        limit: 1 + f
            .blocks
            .iter()
            .flat_map(|b| &b.stmts)
            .filter(|s| matches!(s, Statement::Retain(_)))
            .count(),
        findings: BTreeSet::new(),
    };
    let mut initial = vec![BTreeSet::from([Ticket::Null]); f.locals.len()];
    for &p in &f.params {
        if flow.tracked(p) {
            initial[p.0 as usize] = BTreeSet::from([
                Ticket::Null,
                if f.locals[p.0 as usize].is_take {
                    Ticket::Owned(1)
                } else {
                    Ticket::Borrow
                },
            ]);
        }
    }
    let mut incoming = vec![None; f.blocks.len()];
    incoming[f.entry.0 as usize] = Some(initial);
    let mut pending = VecDeque::from([f.entry]);
    while let Some(id) = pending.pop_front() {
        let bi = id.0 as usize;
        let mut state = incoming[bi].clone().expect("reachable ownership state");
        let block = &f.blocks[bi];
        for (si, stmt) in block.stmts.iter().enumerate() {
            super::token_transfer::statement(&mut flow, &mut state, bi, si, stmt);
        }
        let si = block.stmts.len();
        match &block.terminator {
            Terminator::Return(value) | Terminator::AsyncComplete(value) => {
                if interner.is_rc_tracked(f.ret)
                    && let Some(Operand::Copy(Place::Local(l))) = value {
                        flow.consume(&mut state, *l, bi, si);
                    }
                flow.exit(&state, bi, si);
            }
            Terminator::TailCall { callee, args } => {
                super::token_transfer::call(
                    &mut flow,
                    &mut state,
                    bi,
                    si,
                    &callee.take_params,
                    args,
                );
                flow.exit(&state, bi, si);
            }
            Terminator::Await { dest, future, .. } => {
                // Cancellation drops each owning frame slot once. A borrowed pointer or extra
                // local credit cannot be parked there, even if the normal resume path balances.
                for (index, slot) in state.iter().enumerate() {
                    let local = Local(index as u32);
                    let decl = &f.locals[index];
                    let borrowed_param = f.params.contains(&local) && !decl.is_take;
                    if flow.tracked(local)
                        && !decl.is_cursor
                        && !borrowed_param
                        && slot
                            .iter()
                            .any(|t| matches!(t, Ticket::Borrow | Ticket::Owned(2..)))
                    {
                        flow.error(
                            bi,
                            si,
                            local,
                            "async frame slot must own exactly one cancellation token",
                        );
                    }
                }
                if let Some(dest) = dest.filter(|d| flow.tracked(*d)) {
                    // The child Future owns its result until the resume handoff. The parent
                    // frame retains the child separately; a reused future slot is consumed here.
                    if matches!(future, Operand::Copy(Place::Local(l)) if *l == dest) {
                        flow.consume(&mut state, dest, bi, si);
                    }
                    flow.overwrite(&state, dest, bi, si);
                    state[dest.0 as usize] = BTreeSet::from([Ticket::Owned(1), Ticket::Null]);
                }
            }
            _ => {}
        }
        for successor in block.terminator.successors() {
            let row = &mut incoming[successor.0 as usize];
            let changed = if let Some(previous) = row {
                let mut changed = false;
                for (old, new) in previous.iter_mut().zip(&state) {
                    let size = old.len();
                    old.extend(new);
                    changed |= old.len() != size;
                }
                changed
            } else {
                *row = Some(state.clone());
                true
            };
            if changed {
                pending.push_back(successor);
            }
        }
    }
    flow.findings
        .into_iter()
        .map(|(bi, si, message)| violation(f, bi, si, message))
        .collect()
}
