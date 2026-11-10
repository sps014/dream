//! `Promise.all([a(..), b(..)]).await` over children that can never suspend.
//!
//! `Promise.all`'s poll runs synchronous checks on its parameters and then awaits the
//! `promise_all` combinator, which starts each child in order. A child without an `Await` settles
//! during that start, so the join is the checks followed by every child's synchronous copy in
//! array order. The rewrite performs exactly that sequence in the caller and builds the result
//! array directly, so neither the children nor the join allocate a future.

use super::{Key, mentions};
use crate::{Callee, Local, LocalDecl, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefId, TyKind, TypeId, TypeInterner};
use std::collections::{BTreeMap, BTreeSet};

/// A poll that only calls synchronous functions on its parameters before settling with
/// `promise_all(params[promises])`.
pub(super) struct Join {
    params: Vec<Local>,
    promises: usize,
    checks: Vec<(Callee, Vec<Operand>)>,
}

fn param_index(params: &[Local], o: &Operand) -> Option<usize> {
    match o {
        Operand::Copy(Place::Local(l)) => params.iter().position(|p| p == l),
        _ => None,
    }
}

fn is_marker(s: &Statement) -> bool {
    matches!(s, Statement::SourceLine(_) | Statement::DebugLine(_))
}

pub(super) fn recognize(
    poll: &MirFunction,
    promise_all: &BTreeSet<DefId>,
    asyncs: &BTreeSet<Key>,
    task_users: &BTreeSet<Key>,
) -> Option<Join> {
    let entry = poll.blocks.get(poll.entry.0 as usize)?;
    let Terminator::Await {
        future: Operand::Copy(Place::Local(fut)),
        dest: Some(settled),
        resume,
    } = entry.terminator
    else {
        return None;
    };
    let done = poll.blocks.get(resume.0 as usize)?;
    if !done.stmts.iter().all(is_marker)
        || !matches!(&done.terminator, Terminator::AsyncComplete(Some(Operand::Copy(Place::Local(l)))) if *l == settled)
    {
        return None;
    }
    let mut body = entry.stmts.iter().filter(|s| !is_marker(s));
    let Some(Statement::Assign(Place::Local(target), Rvalue::Call { callee, args })) =
        body.next_back()
    else {
        return None;
    };
    let param = |o: &Operand| param_index(&poll.params, o);
    let promises = match args.as_slice() {
        [arr] if *target == fut && promise_all.contains(&callee.def) => param(arr)?,
        _ => return None,
    };
    let mut used = vec![false; poll.params.len()];
    used[promises] = true;
    let mut checks = Vec::new();
    for s in body {
        let Statement::Call { callee, args } = s else {
            return None;
        };
        let key = (callee.def, callee.args.clone());
        if asyncs.contains(&key) || super::reaches(task_users, &key) {
            return None;
        }
        for a in args {
            if let Operand::Copy(_) = a {
                let i = param(a)?;
                if std::mem::replace(&mut used[i], true) {
                    return None;
                }
            }
        }
        checks.push((callee.clone(), args.clone()));
    }
    Some(Join {
        params: poll.params.clone(),
        promises,
        checks,
    })
}

/// A ready child's construction `local = call stub(args)` at `index` in the block.
struct Child {
    index: usize,
    callee: Callee,
    args: Vec<Operand>,
}

pub(super) fn rewrite(
    poll: &mut MirFunction,
    joins: &BTreeMap<Key, Join>,
    ready: &BTreeMap<Key, Callee>,
    interner: &TypeInterner,
) -> bool {
    // Each rewrite only drops mentions of the locals it removes and adds fresh ones, so the
    // counts stay valid for the remaining blocks.
    let counts = mentions(poll);
    let mut changed = false;
    for bi in 0..poll.blocks.len() {
        if let Some((stmts, resume)) = plan(poll, bi, &counts, joins, ready, interner) {
            poll.blocks[bi].stmts = stmts;
            poll.blocks[bi].terminator = Terminator::Goto(resume);
            changed = true;
        }
    }
    changed
}

/// The block's statements with the join replaced by its synchronous sequence.
fn plan(
    poll: &mut MirFunction,
    bi: usize,
    counts: &[u32],
    joins: &BTreeMap<Key, Join>,
    ready: &BTreeMap<Key, Callee>,
    interner: &TypeInterner,
) -> Option<(Vec<Statement>, crate::BlockId)> {
    let block = &poll.blocks[bi];
    let Terminator::Await {
        future: Operand::Copy(Place::Local(fut)),
        dest: Some(dest),
        resume,
    } = block.terminator
    else {
        return None;
    };
    let single = |l: Local| counts.get(l.0 as usize) == Some(&2);
    let def_of = |l: Local, before: usize| {
        block.stmts[..before]
            .iter()
            .rposition(|s| matches!(s, Statement::Assign(Place::Local(t), _) if *t == l))
    };
    let at = block.stmts.iter().rposition(|s| !is_marker(s))?;
    let Statement::Assign(
        Place::Local(target),
        Rvalue::Call {
            callee,
            args: join_args,
        },
    ) = &block.stmts[at]
    else {
        return None;
    };
    let join = joins.get(&(callee.def, callee.args.clone()))?;
    if *target != fut || !single(fut) || join_args.len() != join.params.len() {
        return None;
    }
    let Operand::Copy(Place::Local(array)) = join_args[join.promises] else {
        return None;
    };
    let array_at = def_of(array, at).filter(|_| single(array))?;
    let Statement::Assign(_, Rvalue::ArrayLit { elems, .. }) = &block.stmts[array_at] else {
        return None;
    };
    let TyKind::Array(elem_ty) = interner.kind(poll.locals[dest.0 as usize].ty) else {
        return None;
    };
    let elem_ty: TypeId = *elem_ty;
    let mut children = Vec::new();
    for e in elems {
        let Operand::Copy(Place::Local(c)) = e else {
            return None;
        };
        let index = def_of(*c, array_at).filter(|_| single(*c))?;
        let Statement::Assign(_, Rvalue::Call { callee, args }) = &block.stmts[index] else {
            return None;
        };
        let direct = ready.get(&(callee.def, callee.args.clone()))?;
        if direct.ret != elem_ty || children.iter().any(|k: &Child| k.index == index) {
            return None;
        }
        // The child now runs at the await, so its arguments must still hold their values there.
        if !args
            .iter()
            .all(|a| matches!(a, Operand::Const(_) | Operand::Copy(Place::Local(_))))
        {
            return None;
        }
        let reads = operand_locals(args);
        if block.stmts[index + 1..]
            .iter()
            .any(|s| matches!(s, Statement::Assign(p, _) if assigned(p).is_some_and(|l| reads.contains(&l))))
        {
            return None;
        }
        children.push(Child {
            index,
            callee: direct.clone(),
            args: args.clone(),
        });
    }
    if children.is_empty() {
        return None;
    }

    let substitute = |o: &Operand| match param_index(&join.params, o) {
        Some(i) => join_args[i].clone(),
        None => o.clone(),
    };
    let checks: Vec<Statement> = join
        .checks
        .iter()
        .map(|(callee, args)| Statement::Call {
            callee: callee.clone(),
            args: args.iter().map(substitute).collect(),
        })
        .collect();
    let removed: BTreeSet<usize> = children
        .iter()
        .map(|c| c.index)
        .chain([array_at, at])
        .collect();
    let mut stmts: Vec<Statement> = block
        .stmts
        .iter()
        .enumerate()
        .filter(|(i, _)| !removed.contains(i))
        .map(|(_, s)| s.clone())
        .collect();
    stmts.extend(checks);
    let mut values = Vec::new();
    for child in children {
        let value = Local(poll.locals.len() as u32);
        poll.locals.push(LocalDecl {
            ty: elem_ty,
            name: None,
            is_ref: false,
            is_take: false,
            is_cursor: false,
            manual_drop: false,
            borrows_refs: false,
        });
        stmts.push(Statement::Assign(
            Place::Local(value),
            Rvalue::Call {
                callee: child.callee,
                args: child.args,
            },
        ));
        values.push(Operand::Copy(Place::Local(value)));
    }
    stmts.push(Statement::Assign(
        Place::Local(dest),
        Rvalue::ArrayLit {
            elem_ty,
            elems: values,
        },
    ));
    Some((stmts, resume))
}

fn assigned(p: &Place) -> Option<Local> {
    match p {
        Place::Local(l) => Some(*l),
        _ => None,
    }
}

fn operand_locals(args: &[Operand]) -> BTreeSet<Local> {
    args.iter()
        .filter_map(|a| match a {
            Operand::Copy(Place::Local(l)) => Some(*l),
            _ => None,
        })
        .collect()
}
