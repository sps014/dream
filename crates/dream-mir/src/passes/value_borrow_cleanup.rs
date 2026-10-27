//! Empty value storage carries no token, including cleanup after a loop's ValueKill.

use super::*;
use std::collections::VecDeque;

pub(super) fn active_cleanup(
    f: &MirFunction,
    member: &impl Fn(Local) -> bool,
) -> BTreeSet<(usize, usize)> {
    let mut incoming = vec![None; f.blocks.len()];
    incoming[f.entry.0 as usize] = Some(f.params.iter().copied().filter(|&l| member(l)).collect());
    let mut queue = VecDeque::from([f.entry.0 as usize]);
    let mut queued = vec![false; f.blocks.len()];
    queued[f.entry.0 as usize] = true;
    while let Some(bi) = queue.pop_front() {
        queued[bi] = false;
        let mut state = incoming[bi].clone().expect("reachable value state");
        for stmt in &f.blocks[bi].stmts {
            step(stmt, &mut state, member);
        }
        for next in f.blocks[bi].terminator.successors() {
            let next = next.0 as usize;
            let changed = match &mut incoming[next] {
                None => {
                    incoming[next] = Some(state.clone());
                    true
                }
                Some(entry) => {
                    let old = entry.len();
                    entry.extend(state.iter().copied());
                    entry.len() != old
                }
            };
            if changed && !queued[next] {
                queued[next] = true;
                queue.push_back(next);
            }
        }
    }
    let mut active = BTreeSet::new();
    for (bi, block) in f.blocks.iter().enumerate() {
        let Some(mut state) = incoming[bi].clone() else {
            continue;
        };
        for (si, stmt) in block.stmts.iter().enumerate() {
            if matches!(stmt, Statement::ValueRetain(l) | Statement::ValueDrop(l) if state.contains(l))
            {
                active.insert((bi, si));
            }
            step(stmt, &mut state, member);
        }
    }
    active
}

fn step(stmt: &Statement, state: &mut BTreeSet<Local>, member: &impl Fn(Local) -> bool) {
    match stmt {
        Statement::Assign(Place::Local(dest), rv) if member(*dest) => {
            let holds = match rv {
                Rvalue::New {
                    ctor: None, args, ..
                } if args.is_empty() => false,
                Rvalue::Use(Operand::Const(Const::Null)) => false,
                Rvalue::Use(Operand::Copy(Place::Local(src))) if member(*src) => {
                    state.contains(src)
                }
                Rvalue::Move { src, .. } if member(*src) => state.contains(src),
                _ => true,
            };
            if holds {
                state.insert(*dest);
            } else {
                state.remove(dest);
            }
        }
        Statement::Assign(Place::Field { base, .. }, _) if member(*base) => {
            state.insert(*base);
        }
        Statement::ValueKill(local) => {
            state.remove(local);
        }
        _ => {}
    }
}
