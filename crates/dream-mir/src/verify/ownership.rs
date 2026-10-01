//! Token death analysis across the function CFG.

use super::operands::{other_stmt_locals, rvalue_local_operands, terminator_reads};
use super::{violation, Violation};
use crate::{BasicBlock, Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

fn rc_local(op: &Operand) -> Option<Local> {
    match op {
        Operand::Copy(Place::Local(l)) => Some(*l),
        _ => None,
    }
}

pub(super) fn check_rc_types(
    f: &MirFunction,
    interner: &TypeInterner,
    bi: usize,
    block: &BasicBlock,
    out: &mut Vec<Violation>,
) {
    for (si, s) in block.stmts.iter().enumerate() {
        let (op, what) = match s {
            Statement::Retain(o) => (o, "retain"),
            Statement::Release(o) => (o, "release"),
            _ => continue,
        };
        let Some(l) = rc_local(op) else { continue };
        let Some(decl) = f.locals.get(l.0 as usize) else {
            out.push(violation(
                f,
                bi,
                si,
                format!("{what} of undeclared local _{}", l.0),
            ));
            continue;
        };
        if !interner.is_rc_tracked(decl.ty) {
            out.push(violation(
                f,
                bi,
                si,
                format!("{what} of non-RC local _{}", l.0),
            ));
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dead {
    Released,
}

fn check_block_paths(
    f: &MirFunction,
    bi: usize,
    block: &BasicBlock,
    single_token: &BTreeSet<u32>,
    incoming: &BTreeMap<u32, Dead>,
    out: &mut Vec<Violation>,
) {
    let mut dead = incoming.clone();
    for (si, s) in block.stmts.iter().enumerate() {
        for (&l, &how) in &dead {
            if !crate::passes::stmt_reads_local(s, l) {
                continue;
            }
            let msg = match (how, s) {
                (Dead::Released, Statement::Release(_)) => {
                    format!("_{l} released twice with no retain or redefinition between")
                }
                (Dead::Released, _) => format!("_{l} used after its only token was released"),
            };
            out.push(violation(f, bi, si, msg));
        }
        if let Some(d) = defined_local(s) {
            dead.remove(&d);
        }
        if let Statement::Release(o) = s {
            if let Some(l) = rc_local(o) {
                if single_token.contains(&l.0) {
                    mark(&mut dead, l.0, Dead::Released);
                }
            }
        }
    }
    let term_reads = terminator_reads(&block.terminator);
    for (&l, &how) in &dead {
        if term_reads.contains(&l) {
            let msg = match how {
                Dead::Released => {
                    format!("_{l} used by terminator after its only token was released")
                }
            };
            out.push(violation(f, bi, block.stmts.len(), msg));
        }
    }
}

fn mark(dead: &mut BTreeMap<u32, Dead>, l: u32, how: Dead) {
    dead.insert(l, how);
}

pub(super) fn check_paths(f: &MirFunction, out: &mut Vec<Violation>) {
    let single_token = single_token_locals(f);
    let mut incoming = vec![None; f.blocks.len()];
    incoming[f.entry.0 as usize] = Some(BTreeMap::new());
    let mut pending = VecDeque::from([f.entry]);
    while let Some(bi) = pending.pop_front() {
        let mut state = incoming[bi.0 as usize]
            .clone()
            .expect("queued reachable block");
        for stmt in &f.blocks[bi.0 as usize].stmts {
            if let Some(d) = defined_local(stmt) {
                state.remove(&d);
            }
            if let Statement::Release(op) = stmt {
                if let Some(l) = rc_local(op).filter(|l| single_token.contains(&l.0)) {
                    mark(&mut state, l.0, Dead::Released);
                }
            }
        }
        for successor in f.blocks[bi.0 as usize].terminator.successors() {
            let row = &mut incoming[successor.0 as usize];
            let mut changed = row.is_none();
            let joined = row.get_or_insert_with(BTreeMap::new);
            for (&local, &death) in &state {
                // A use is invalid if any incoming path has lost its last owner.
                // The finite may-dead set needs no iteration cap.
                if let std::collections::btree_map::Entry::Vacant(entry) = joined.entry(local) {
                    entry.insert(death);
                    changed = true;
                }
            }
            if changed {
                pending.push_back(successor);
            }
        }
    }
    for (bi, block) in f.blocks.iter().enumerate() {
        if let Some(state) = &incoming[bi] {
            check_block_paths(f, bi, block, &single_token, state, out);
        }
    }
}

fn defined_local(s: &Statement) -> Option<u32> {
    match s {
        Statement::Assign(Place::Local(d), _) => Some(d.0),
        _ => None,
    }
}

/// Locals whose whole copy/cast/move alias class never gains a second owner: no member is
/// retained, stored anywhere but a plain local, handed to a call/constructor/union/array, or is a
/// parameter. Every definition of such a local yields at most one token.
fn single_token_locals(f: &MirFunction) -> BTreeSet<u32> {
    let n = f.locals.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let union = |parent: &mut Vec<usize>, a: u32, b: u32| {
        let (a, b) = (a as usize, b as usize);
        if a >= n || b >= n {
            return;
        }
        let (ra, rb) = (find(parent, a), find(parent, b));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    };
    let mut shared: BTreeSet<u32> = f.params.iter().map(|p| p.0).collect();
    for block in &f.blocks {
        for s in &block.stmts {
            match s {
                Statement::Retain(o) => {
                    if let Some(l) = rc_local(o) {
                        shared.insert(l.0);
                    }
                }
                Statement::Assign(Place::Local(d), rv) => match rv {
                    Rvalue::Use(Operand::Copy(Place::Local(s)))
                    | Rvalue::Cast(Operand::Copy(Place::Local(s)), _, _)
                    | Rvalue::Move { src: s, .. } => union(&mut parent, d.0, s.0),
                    Rvalue::Select {
                        then_val, else_val, ..
                    } => {
                        for l in [then_val, else_val].iter().filter_map(|o| rc_local(o)) {
                            union(&mut parent, d.0, l.0);
                        }
                    }
                    Rvalue::UnionField { base, .. } => {
                        if let Some(b) = rc_local(base) {
                            union(&mut parent, d.0, b.0);
                        }
                    }
                    Rvalue::Use(_)
                    | Rvalue::Cast(..)
                    | Rvalue::Binary(..)
                    | Rvalue::CheckedBinary(..)
                    | Rvalue::Unary(..)
                    | Rvalue::CheckedNeg(_)
                    | Rvalue::Discriminant { .. }
                    | Rvalue::IsType(..)
                    | Rvalue::ArrayLen(_)
                    | Rvalue::StrLen(_)
                    | Rvalue::StrByteSize(_)
                    | Rvalue::CharAt(..)
                    | Rvalue::ByteAt(..)
                    | Rvalue::StrBytes(_)
                    | Rvalue::LoadU8(..)
                    | Rvalue::LoadU16(..)
                    | Rvalue::HashCode(_)
                    | Rvalue::EnumName { .. } => {}
                    _ => shared.extend(rvalue_local_operands(rv)),
                },
                Statement::Assign(_, rv) => shared.extend(rvalue_local_operands(rv)),
                Statement::Release(_)
                | Statement::Nop
                | Statement::DebugLine(_)
                | Statement::SourceLine(_) => {}
                other => shared.extend(other_stmt_locals(other)),
            }
        }
        match &block.terminator {
            Terminator::TailCall { args, .. } => {
                shared.extend(args.iter().filter_map(rc_local).map(|l| l.0));
            }
            Terminator::Await { dest: Some(d), .. } => {
                shared.insert(d.0);
            }
            _ => {}
        }
    }
    let shared_roots: BTreeSet<usize> = shared
        .iter()
        .filter(|l| (**l as usize) < n)
        .map(|l| find(&mut parent, *l as usize))
        .collect();
    (0..n)
        .filter(|l| !shared_roots.contains(&find(&mut parent, *l)))
        .map(|l| l as u32)
        .collect()
}
