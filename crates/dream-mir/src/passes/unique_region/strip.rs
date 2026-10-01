use super::*;

#[cfg(test)]
mod tests;

/// Drop enter/leave pairs when a later pass merged a payload use (call, to_string, return)
/// after `RegionLeave` without redefining the RC local.
pub fn strip_escaped_regions(mir: &mut Mir, interner: &TypeInterner) -> bool {
    let mut changed = false;
    for f in &mut mir.functions {
        if strip_escaped_fn(f, interner) {
            if cfg!(debug_assertions) {
                crate::internal_error!(
                    "escaped inferred region in {} after function passes",
                    f.name
                );
            }
            eprintln!(
                "warning: removed escaped inferred region in {} after function passes",
                f.name
            );
            changed = true;
        }
    }
    changed
}

pub(super) fn strip_escaped_fn(f: &mut MirFunction, interner: &TypeInterner) -> bool {
    let mut stack = Vec::new();
    let mut drop_at: IndexSet<(usize, usize)> = IndexSet::new();
    for (bi, block) in f.blocks.iter().enumerate() {
        for (si, stmt) in block.stmts.iter().enumerate() {
            match stmt {
                Statement::RegionEnter => stack.push((bi, si)),
                Statement::RegionLeave => {
                    if let Some(enter) = stack.pop() {
                        let tainted = region_body_defs(f, interner, enter);
                        if rc_use_after_leave(f, bi, si, &tainted) {
                            drop_at.insert(enter);
                            drop_at.insert((bi, si));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if drop_at.is_empty() {
        return false;
    }
    for (bi, block) in f.blocks.iter_mut().enumerate() {
        let mut si = block.stmts.len();
        while si > 0 {
            si -= 1;
            if drop_at.contains(&(bi, si)) {
                block.stmts.remove(si);
            }
        }
    }
    true
}

/// RC locals assigned a non-null value somewhere between `enter` and the first `RegionLeave` on
/// each path — the only locals that can hold region memory once the region rewinds. Locals
/// defined solely outside the body (e.g. a loop's `Stopwatch` set up before the region) cannot.
pub(super) fn region_body_defs(
    f: &MirFunction,
    interner: &TypeInterner,
    (enter_bi, enter_si): (usize, usize),
) -> BTreeSet<u32> {
    let mut defs = BTreeSet::new();
    let mut seen = IndexSet::new();
    let mut stack = vec![(enter_bi, enter_si + 1)];
    while let Some((bi, si0)) = stack.pop() {
        if si0 == 0 && !seen.insert(bi) {
            continue;
        }
        let Some(block) = f.blocks.get(bi) else {
            continue;
        };
        let mut left = false;
        for stmt in block.stmts.iter().skip(si0) {
            match stmt {
                Statement::RegionLeave => {
                    left = true;
                    break;
                }
                Statement::Assign(Place::Local(_), Rvalue::Use(Operand::Const(Const::Null))) => {}
                Statement::Assign(Place::Local(d), _)
                    if f.locals
                        .get(d.0 as usize)
                        .is_some_and(|l| interner.is_rc_tracked(l.ty)) =>
                {
                    defs.insert(d.0);
                }
                _ => {}
            }
        }
        if left {
            continue;
        }
        if let Terminator::Await { dest: Some(d), .. } = &block.terminator {
            defs.insert(d.0);
        }
        for s in block.terminator.successors() {
            stack.push((s.0 as usize, 0));
        }
    }
    defs
}

pub(super) fn rc_use_after_leave(
    f: &MirFunction,
    leave_bi: usize,
    leave_si: usize,
    tainted: &BTreeSet<u32>,
) -> bool {
    let mut incoming: Vec<Option<BTreeSet<u32>>> = vec![None; f.blocks.len()];
    let mut pending = std::collections::VecDeque::from([(leave_bi, leave_si + 1, tainted.clone())]);
    while let Some((bi, si0, mut dangling)) = pending.pop_front() {
        let Some(block) = f.blocks.get(bi) else {
            continue;
        };
        for stmt in block.stmts.iter().skip(si0) {
            if rc_stmt_escapes(stmt, &dangling) {
                return true;
            }
            if let Statement::Assign(Place::Local(d), _) = stmt {
                dangling.remove(&d.0);
            }
        }
        if rc_term_escapes(&block.terminator, &dangling) {
            return true;
        }
        if let Terminator::Await { dest: Some(d), .. } = &block.terminator {
            dangling.remove(&d.0);
        }
        for successor in block.terminator.successors() {
            let Some(row) = incoming.get_mut(successor.0 as usize) else {
                continue;
            };
            let changed = row.as_ref().is_none_or(|old| !dangling.is_subset(old));
            if changed {
                let joined = row.get_or_insert_with(BTreeSet::new);
                joined.extend(dangling.iter().copied());
                pending.push_back((successor.0 as usize, 0, joined.clone()));
            }
        }
    }
    false
}

pub(super) fn rc_stmt_escapes(stmt: &Statement, tainted: &BTreeSet<u32>) -> bool {
    match stmt {
        Statement::Retain(_) | Statement::Release(_) => false,
        Statement::Assign(Place::Local(_), Rvalue::Use(Operand::Const(Const::Null))) => false,
        Statement::RegionEnter | Statement::RegionLeave => false,
        _ => tainted
            .iter()
            .any(|i| crate::passes::rc::stmt_reads_local(stmt, *i)),
    }
}

pub(super) fn rc_term_escapes(term: &Terminator, tainted: &BTreeSet<u32>) -> bool {
    let mut live = IndexSet::new();
    match term {
        Terminator::Return(Some(o)) | Terminator::AsyncComplete(Some(o)) => {
            operand_locals(o, &mut live);
        }
        Terminator::If { cond, .. } => operand_locals(cond, &mut live),
        Terminator::Switch { value, .. } => operand_locals(value, &mut live),
        Terminator::TailCall { args, .. } => {
            for a in args {
                operand_locals(a, &mut live);
            }
        }
        Terminator::Await { future, .. } => operand_locals(future, &mut live),
        _ => {}
    }
    live.iter().any(|i| tainted.contains(i))
}

pub(super) fn operand_locals(op: &Operand, live: &mut IndexSet<u32>) {
    if let Operand::Copy(place) = op {
        match place {
            Place::Local(l) => {
                live.insert(l.0);
            }
            Place::Field { base, .. } | Place::Deref { ptr: base, .. } => {
                live.insert(base.0);
            }
            Place::Index { base, index, .. } => {
                live.insert(base.0);
                operand_locals(index, live);
            }
            Place::Global(_) => {}
        }
    }
}
