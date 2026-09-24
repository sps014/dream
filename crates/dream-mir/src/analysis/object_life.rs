//! Static reference count of one allocation whose every holder is a local of its function.
//!
//! Given the alias class of `o = New …` (see [`super::escape`]), every definition of a member must
//! be that `New`, a copy of a member, or null, so each member either holds the object, holds
//! null, or is a stale alias of an instance already dead. A forward dataflow then tracks the
//! object's count through the members' `Retain` / `Release` / `ReleaseUnique`, requiring every
//! path into a block to agree on the count and on which members hold it. The statements that drop the last count are the object's deaths;
//! a fresh `New` is only allowed once the previous instance is dead, and every return must leave
//! it dead. Handing a member to a `take` parameter moves a count out of sight and is refused.

use crate::{Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use std::collections::HashMap;

/// Where one allocation's counts go.
pub(crate) struct Lifetime {
    /// `(block, stmt, member)` of each `Release` / `ReleaseUnique` that drops the last count.
    pub deaths: Vec<(usize, usize, Local)>,
    /// `(block, stmt)` of every other RC statement on a member.
    pub rc_ops: Vec<(usize, usize)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Hold {
    Null,
    Obj,
    Stale,
}

#[derive(Clone, PartialEq, Eq, Debug)]
struct State {
    count: u32,
    hold: Vec<Hold>,
}

/// Counts beyond this are treated as unbounded (a retain inside a loop).
const MAX_COUNT: u32 = 64;

enum Rc {
    Other,
    Death(Local),
    Op,
}

pub(crate) fn lifetime(f: &MirFunction, members: &[Local], new_local: Local) -> Option<Lifetime> {
    let pos: HashMap<Local, usize> = members.iter().enumerate().map(|(i, &m)| (m, i)).collect();
    if members.iter().any(|m| {
        f.params.contains(m) || f.locals[m.0 as usize].is_ref
    }) || !pos.contains_key(&new_local)
    {
        return None;
    }
    let mut news = 0;
    for b in &f.blocks {
        for s in &b.stmts {
            if let Statement::Assign(Place::Local(d), Rvalue::New { .. }) = s {
                if pos.contains_key(d) {
                    news += 1;
                    if *d != new_local {
                        return None;
                    }
                }
            }
            if gives_count_away(s, &pos) {
                return None;
            }
        }
        match &b.terminator {
            Terminator::Await { dest: Some(d), .. } if pos.contains_key(d) => return None,
            Terminator::TailCall { .. } => return None,
            _ => {}
        }
    }
    if news != 1 {
        return None;
    }

    let n = f.blocks.len();
    let mut entry_state: Vec<Option<State>> = vec![None; n];
    entry_state[f.entry.0 as usize] = Some(State {
        count: 0,
        hold: vec![Hold::Null; members.len()],
    });
    let mut work = vec![f.entry.0 as usize];
    while let Some(bi) = work.pop() {
        let mut st = entry_state[bi].clone().expect("queued with a state");
        for s in &f.blocks[bi].stmts {
            step(s, &pos, &mut st)?;
        }
        let term = &f.blocks[bi].terminator;
        if matches!(term, Terminator::Return(_) | Terminator::AsyncComplete(_)) && st.count != 0 {
            return None;
        }
        for succ in term.successors() {
            let si = succ.0 as usize;
            let joined = match &entry_state[si] {
                None => st.clone(),
                Some(prev) => join(prev, &st)?,
            };
            if entry_state[si].as_ref() != Some(&joined) {
                entry_state[si] = Some(joined);
                work.push(si);
            }
        }
    }

    let mut out = Lifetime {
        deaths: Vec::new(),
        rc_ops: Vec::new(),
    };
    for (bi, b) in f.blocks.iter().enumerate() {
        let Some(mut st) = entry_state[bi].clone() else {
            for (si, s) in b.stmts.iter().enumerate() {
                if rc_member(s, &pos).is_some() {
                    out.rc_ops.push((bi, si));
                }
            }
            continue;
        };
        for (si, s) in b.stmts.iter().enumerate() {
            match step(s, &pos, &mut st).expect("replays the accepted fixpoint") {
                Rc::Other => {}
                Rc::Death(m) => out.deaths.push((bi, si, m)),
                Rc::Op => out.rc_ops.push((bi, si)),
            }
        }
    }
    Some(out)
}

/// Paths must agree on the count and on who holds the object; a null and a stale alias join
/// as stale (no RC statement may touch it).
fn join(a: &State, b: &State) -> Option<State> {
    if a.count != b.count {
        return None;
    }
    let hold = a
        .hold
        .iter()
        .zip(&b.hold)
        .map(|(&x, &y)| match (x, y) {
            _ if x == y => Some(x),
            (Hold::Null, Hold::Stale) | (Hold::Stale, Hold::Null) => Some(Hold::Stale),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(State {
        count: a.count,
        hold,
    })
}

fn member_of(op: &Operand, pos: &HashMap<Local, usize>) -> Option<Local> {
    match op {
        Operand::Copy(Place::Local(l)) if pos.contains_key(l) => Some(*l),
        _ => None,
    }
}

fn rc_member(s: &Statement, pos: &HashMap<Local, usize>) -> Option<Local> {
    match s {
        Statement::Retain(op) | Statement::Release(op) | Statement::ReleaseUnique(op) => {
            member_of(op, pos)
        }
        _ => None,
    }
}

fn step(s: &Statement, pos: &HashMap<Local, usize>, st: &mut State) -> Option<Rc> {
    let kill = |st: &mut State| {
        for h in &mut st.hold {
            if *h == Hold::Obj {
                *h = Hold::Stale;
            }
        }
    };
    match s {
        Statement::Assign(Place::Local(d), rv) if pos.contains_key(d) => {
            let hold = match rv {
                Rvalue::New { .. } => {
                    if st.count != 0 {
                        return None;
                    }
                    st.count = 1;
                    Hold::Obj
                }
                Rvalue::Use(Operand::Const(crate::Const::Null)) => Hold::Null,
                Rvalue::Use(op) | Rvalue::Cast(op, _, _) => st.hold[pos[&member_of(op, pos)?]],
                Rvalue::Move { src, .. } => st.hold[*pos.get(src)?],
                _ => return None,
            };
            st.hold[pos[d]] = hold;
            Some(Rc::Other)
        }
        Statement::Retain(op) => {
            let Some(m) = member_of(op, pos) else {
                return Some(Rc::Other);
            };
            if st.hold[pos[&m]] != Hold::Obj || st.count == 0 || st.count >= MAX_COUNT {
                return None;
            }
            st.count += 1;
            Some(Rc::Op)
        }
        Statement::Release(op) => {
            let Some(m) = member_of(op, pos) else {
                return Some(Rc::Other);
            };
            match st.hold[pos[&m]] {
                Hold::Null => Some(Rc::Op),
                Hold::Stale => None,
                Hold::Obj => {
                    st.count -= 1;
                    if st.count == 0 {
                        kill(st);
                        Some(Rc::Death(m))
                    } else {
                        Some(Rc::Op)
                    }
                }
            }
        }
        Statement::ReleaseUnique(op) => {
            let Some(m) = member_of(op, pos) else {
                return Some(Rc::Other);
            };
            if st.hold[pos[&m]] != Hold::Obj || st.count != 1 {
                return None;
            }
            st.count = 0;
            kill(st);
            Some(Rc::Death(m))
        }
        _ => Some(Rc::Other),
    }
}

/// A member passed where the callee adopts a count.
fn gives_count_away(s: &Statement, pos: &HashMap<Local, usize>) -> bool {
    let taken = |takes: &[bool], args: &[Operand]| {
        args.iter()
            .enumerate()
            .any(|(i, a)| takes.get(i).copied().unwrap_or(false) && member_of(a, pos).is_some())
    };
    match s {
        Statement::Call { callee, args }
        | Statement::Assign(_, Rvalue::Call { callee, args }) => taken(&callee.take_params, args),
        Statement::Assign(
            _,
            Rvalue::New {
                ctor: Some(c),
                args,
                ..
            },
        ) => taken(&c.take_params, args),
        Statement::Assign(Place::Local(d), Rvalue::Select { .. }) => pos.contains_key(d),
        _ => false,
    }
}
