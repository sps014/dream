//! Static reference count of one allocation whose every holder is a local of its function.
//!
//! Given the alias class of `o = New …` (see [`super::escape`]), every definition of a member must
//! be that `New`, a copy of a member, or null, so each member either holds the object, holds
//! null, or is a stale alias of an instance already dead. A forward dataflow then tracks the
//! object's count through the members' `Retain`, preserving alternative holder states at joins.
//! A release must have the same destruction role in every incoming state. The statements that
//! drop the last count are the object's deaths;
//! a fresh `New` is only allowed once the previous instance is dead, and every return must leave
//! it dead. Handing a member to a `take` parameter moves a count out of sight and is refused.

use crate::{Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use indexmap::IndexMap as HashMap;
use std::collections::BTreeSet;

/// Where one allocation's counts go.
pub(crate) struct Lifetime {
    /// `(block, stmt, member)` of each `Release`  that drops the last count.
    pub deaths: Vec<(usize, usize, Local)>,
    /// `(block, stmt)` of every other RC statement on a member.
    pub rc_ops: Vec<(usize, usize)>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Hold {
    Null,
    Obj,
    Stale,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct State {
    count: u32,
    hold: Vec<Hold>,
}

/// Counts beyond this are treated as unbounded (a retain inside a loop).
const MAX_COUNT: u32 = 64;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Rc {
    Other,
    Death(Local),
    Op,
}

pub(crate) fn lifetime(f: &MirFunction, members: &[Local], new_local: Local) -> Option<Lifetime> {
    let pos: HashMap<Local, usize> = members.iter().enumerate().map(|(i, &m)| (m, i)).collect();
    if members
        .iter()
        .any(|m| f.params.contains(m) || f.locals[m.0 as usize].is_ref)
        || !pos.contains_key(&new_local)
    {
        return None;
    }
    let mut news = 0;
    for b in &f.blocks {
        for s in &b.stmts {
            if let Statement::Assign(Place::Local(d), Rvalue::New { .. }) = s
                && pos.contains_key(d)
            {
                news += 1;
                if *d != new_local {
                    return None;
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
    let mut entry_state = vec![BTreeSet::new(); n];
    entry_state[f.entry.0 as usize].insert(State {
        count: 0,
        hold: vec![Hold::Null; members.len()],
    });
    let mut work = vec![f.entry.0 as usize];
    while let Some(bi) = work.pop() {
        let mut states = entry_state[bi].clone();
        for s in &f.blocks[bi].stmts {
            states = advance(s, &pos, &states)?.0;
        }
        let term = &f.blocks[bi].terminator;
        if matches!(term, Terminator::Return(_) | Terminator::AsyncComplete(_))
            && states.iter().any(|st| st.count != 0)
        {
            return None;
        }
        for succ in term.successors() {
            let si = succ.0 as usize;
            let before = entry_state[si].len();
            entry_state[si].extend(states.iter().cloned());
            if entry_state[si].len() != before {
                work.push(si);
            }
        }
    }

    let mut out = Lifetime {
        deaths: Vec::new(),
        rc_ops: Vec::new(),
    };
    for (bi, b) in f.blocks.iter().enumerate() {
        let mut states = entry_state[bi].clone();
        if states.is_empty() {
            for (si, s) in b.stmts.iter().enumerate() {
                if rc_member(s, &pos).is_some() {
                    out.rc_ops.push((bi, si));
                }
            }
            continue;
        }
        for (si, s) in b.stmts.iter().enumerate() {
            let (next, role) = advance(s, &pos, &states)?;
            states = next;
            match role {
                Rc::Other => {}
                Rc::Death(m) => out.deaths.push((bi, si, m)),
                Rc::Op => out.rc_ops.push((bi, si)),
            }
        }
    }
    Some(out)
}

fn advance(
    stmt: &Statement,
    pos: &HashMap<Local, usize>,
    states: &BTreeSet<State>,
) -> Option<(BTreeSet<State>, Rc)> {
    let mut next = BTreeSet::new();
    let mut role = None;
    for state in states {
        let alternatives = match stmt {
            Statement::Assign(
                place @ Place::Local(d),
                Rvalue::Select {
                    then_val, else_val, ..
                },
            ) if pos.contains_key(d) => vec![
                Statement::Assign(place.clone(), Rvalue::Use(then_val.clone())),
                Statement::Assign(place.clone(), Rvalue::Use(else_val.clone())),
            ],
            _ => vec![stmt.clone()],
        };
        for alternative in alternatives {
            let mut state = state.clone();
            let current = step(&alternative, pos, &mut state)?;
            // Cleanup is unconditional at this MIR site; choosing a runtime branch here
            // would replace the ownership proof with a new runtime ownership test.
            if role.is_some_and(|previous| previous != current) {
                return None;
            }
            role = Some(current);
            next.insert(state);
        }
    }
    Some((next, role.unwrap_or(Rc::Other)))
}

fn member_of(op: &Operand, pos: &HashMap<Local, usize>) -> Option<Local> {
    match op {
        Operand::Copy(Place::Local(l)) if pos.contains_key(l) => Some(*l),
        _ => None,
    }
}

fn rc_member(s: &Statement, pos: &HashMap<Local, usize>) -> Option<Local> {
    match s {
        Statement::Retain(op) | Statement::Release(op) => member_of(op, pos),
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
        Statement::Call { callee, args } | Statement::Assign(_, Rvalue::Call { callee, args }) => {
            taken(&callee.take_params, args)
        }
        Statement::Assign(
            _,
            Rvalue::New {
                ctor: Some(c),
                args,
                ..
            },
        ) => taken(&c.take_params, args),
        _ => false,
    }
}
