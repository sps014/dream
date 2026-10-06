//! Exact path balances for closed, retained alias families with one allocation site.
//! Container/call handoffs are excluded: their counts need ownership facts, not pointer equality.

use super::operands::{operand_locals, other_stmt_locals, rvalue_local_operands, terminator_reads};
use super::{violation, Violation};
use crate::{Const, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

fn local(op: &Operand) -> Option<u32> {
    match op {
        Operand::Copy(Place::Local(l)) => Some(l.0),
        _ => None,
    }
}

fn alias(rv: &Rvalue) -> Option<u32> {
    match rv {
        Rvalue::Use(op) | Rvalue::Cast(op, _, _) => local(op),
        Rvalue::Move { src, .. } => Some(src.0),
        _ => None,
    }
}

fn birth(rv: &Rvalue) -> bool {
    matches!(rv, Rvalue::New { ctor: None, .. } | Rvalue::ArrayNew { .. })
}

struct Family {
    locals: BTreeMap<u32, usize>,
    birth: (usize, usize),
    limit: usize,
}

fn families(f: &MirFunction, interner: &TypeInterner) -> Vec<Family> {
    if f.is_async
        || f.blocks.iter().any(|b| {
            b.stmts
                .iter()
                .any(|s| matches!(s, Statement::RegionEnter | Statement::RegionLeave))
        })
    {
        return Vec::new();
    }
    let mut parent: Vec<usize> = (0..f.locals.len()).collect();
    fn root(parent: &mut [usize], local: usize) -> usize {
        let mut current = local;
        while parent[current] != current {
            parent[current] = parent[parent[current]];
            current = parent[current];
        }
        current
    }
    for block in &f.blocks {
        for stmt in &block.stmts {
            if let Statement::Assign(Place::Local(dest), rv) = stmt
                && let Some(src) = alias(rv) {
                    let a = root(&mut parent, dest.0 as usize);
                    let b = root(&mut parent, src as usize);
                    parent[a.max(b)] = a.min(b);
                }
        }
    }
    let mut excluded: BTreeSet<u32> = f.params.iter().map(|p| p.0).collect();
    let mut births: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
    let mut retains: BTreeMap<usize, usize> = BTreeMap::new();
    for (index, decl) in f.locals.iter().enumerate() {
        if !interner.is_rc_tracked(decl.ty) || decl.is_cursor {
            excluded.insert(index as u32);
        }
    }
    for (bi, block) in f.blocks.iter().enumerate() {
        for (si, stmt) in block.stmts.iter().enumerate() {
            match stmt {
                Statement::Assign(Place::Local(dest), rv) => {
                    if birth(rv) {
                        births
                            .entry(root(&mut parent, dest.0 as usize))
                            .or_default()
                            .push((bi, si));
                        excluded.extend(rvalue_local_operands(rv));
                    } else if !interner.is_rc_tracked(f.locals[dest.0 as usize].ty)
                        && crate::passes::rc::is_pure_rvalue(rv)
                    {
                        // Scalar inspection reads the object without handing off a token.
                    } else if alias(rv).is_none()
                        && !matches!(rv, Rvalue::Use(Operand::Const(Const::Null)))
                    {
                        excluded.insert(dest.0);
                        excluded.extend(rvalue_local_operands(rv));
                    }
                }
                Statement::Retain(op) => {
                    if let Some(l) = local(op) {
                        *retains.entry(root(&mut parent, l as usize)).or_default() += 1;
                    } else {
                        let mut reads = Vec::new();
                        operand_locals(op, &mut reads);
                        excluded.extend(reads);
                    }
                }
                Statement::Release(op) => {
                    if local(op).is_none() {
                        let mut reads = Vec::new();
                        operand_locals(op, &mut reads);
                        excluded.extend(reads);
                    }
                }
                Statement::Nop
                | Statement::DebugLine(_)
                | Statement::SourceLine(_)
                | Statement::RegionEnter
                | Statement::RegionLeave => {}
                Statement::Assign(_, rv) => excluded.extend(rvalue_local_operands(rv)),
                other => excluded.extend(other_stmt_locals(other)),
            }
        }
        if !matches!(
            block.terminator,
            Terminator::Return(_)
                | Terminator::Goto(_)
                | Terminator::If { .. }
                | Terminator::Switch { .. }
                | Terminator::Unreachable
        ) {
            excluded.extend(terminator_reads(&block.terminator));
        }
    }
    let excluded: BTreeSet<usize> = excluded
        .into_iter()
        .map(|l| root(&mut parent, l as usize))
        .collect();
    let mut groups: BTreeMap<usize, BTreeMap<u32, usize>> = BTreeMap::new();
    for local in 0..f.locals.len() {
        let group = root(&mut parent, local);
        let row = groups.entry(group).or_default();
        row.insert(local as u32, row.len());
    }
    groups
        .into_iter()
        .filter_map(|(group, locals)| {
            let sites = births.get(&group)?;
            let retains = *retains.get(&group)?;
            if excluded.contains(&group) || sites.len() != 1 {
                return None;
            }
            Some(Family {
                locals,
                birth: sites[0],
                limit: retains + 1,
            })
        })
        .collect()
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct State {
    count: usize,
    bound: Vec<Binding>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Binding {
    Null,
    Live,
    Dead,
}

pub(super) fn check(f: &MirFunction, interner: &TypeInterner, out: &mut Vec<Violation>) {
    for family in families(f, interner) {
        check_family(f, &family, out);
    }
}

fn check_family(f: &MirFunction, family: &Family, out: &mut Vec<Violation>) {
    let initial = State {
        count: 0,
        bound: vec![Binding::Null; family.locals.len()],
    };
    let mut incoming = vec![BTreeSet::new(); f.blocks.len()];
    incoming[f.entry.0 as usize].insert(initial.clone());
    let mut pending = VecDeque::from([(f.entry, initial)]);
    let mut findings = BTreeSet::new();
    while let Some((bi, mut state)) = pending.pop_front() {
        let block = &f.blocks[bi.0 as usize];
        let mut valid = true;
        for (si, stmt) in block.stmts.iter().enumerate() {
            let reads = match stmt {
                Statement::Assign(place, rv) => {
                    let mut reads = rvalue_local_operands(rv);
                    if !matches!(place, Place::Local(_)) {
                        operand_locals(&Operand::Copy(place.clone()), &mut reads);
                    }
                    reads
                }
                Statement::Retain(op) | Statement::Release(op) => local(op).into_iter().collect(),
                _ => other_stmt_locals(stmt),
            };
            if reads.iter().any(|l| {
                family.locals.get(l).is_some_and(|&i| {
                    state.bound[i] == Binding::Dead
                        || (state.count == 0 && state.bound[i] == Binding::Live)
                })
            }) {
                findings.insert((
                    bi.0 as usize,
                    si,
                    "shared alias used after its last RC token was released",
                ));
                valid = false;
                break;
            }
            match stmt {
                Statement::Assign(Place::Local(dest), rv) => {
                    let Some(&index) = family.locals.get(&dest.0) else {
                        continue;
                    };
                    if (bi.0 as usize, si) == family.birth {
                        if state.count != 0 {
                            findings.insert((
                                bi.0 as usize,
                                si,
                                "shared RC tokens survive allocation-site re-entry",
                            ));
                            valid = false;
                            break;
                        }
                        // Old aliases must already have no remaining uses: their address is not
                        // the new generation, even when the same allocation site executes again.
                        for binding in &mut state.bound {
                            if *binding == Binding::Live {
                                *binding = Binding::Dead;
                            }
                        }
                        state.count = 1;
                        state.bound[index] = Binding::Live;
                    } else if let Some(src) = alias(rv) {
                        state.bound[index] = state.bound[family.locals[&src]];
                        if matches!(rv, Rvalue::Move { .. }) && src != dest.0 {
                            state.bound[family.locals[&src]] = Binding::Null;
                        }
                    } else {
                        state.bound[index] = Binding::Null;
                    }
                }
                Statement::Retain(op) | Statement::Release(op) => {
                    let Some(index) = local(op).and_then(|l| family.locals.get(&l).copied()) else {
                        continue;
                    };
                    if state.bound[index] != Binding::Live {
                        continue;
                    }
                    if matches!(stmt, Statement::Retain(_)) {
                        if state.count == family.limit {
                            // More live counts than all static retains plus the birth requires a
                            // positive-balance cycle. Stop that path instead of widening to "safe".
                            findings.insert((
                                bi.0 as usize,
                                si,
                                "shared RC tokens accumulate across a loop",
                            ));
                            valid = false;
                            break;
                        }
                        state.count += 1;
                    } else {
                        state.count -= 1;
                    }
                }
                _ => {}
            }
        }
        if !valid {
            continue;
        }
        let reads = terminator_reads(&block.terminator);
        if reads.iter().any(|l| {
            family.locals.get(l).is_some_and(|&i| {
                state.bound[i] == Binding::Dead
                    || (state.count == 0 && state.bound[i] == Binding::Live)
            })
        }) {
            findings.insert((
                bi.0 as usize,
                block.stmts.len(),
                "shared alias returned or read after its last RC token was released",
            ));
            continue;
        }
        if let Terminator::Return(value) = &block.terminator
            && value
                .as_ref()
                .and_then(local)
                .and_then(|l| family.locals.get(&l))
                .is_some_and(|&i| state.bound[i] == Binding::Live)
            {
                state.count -= 1;
            }
        if state.count != 0
            && matches!(
                block.terminator,
                Terminator::Return(_) | Terminator::TailCall { .. }
            )
        {
            findings.insert((
                bi.0 as usize,
                block.stmts.len(),
                "shared RC tokens remain at function return",
            ));
        }
        for successor in block.terminator.successors() {
            if incoming[successor.0 as usize].insert(state.clone()) {
                pending.push_back((successor, state.clone()));
            }
        }
    }
    for (bi, si, msg) in findings {
        out.push(violation(
            f,
            bi,
            si,
            format!(
                "{msg} (allocation bb{}[{}])",
                family.birth.0, family.birth.1
            ),
        ));
    }
}
