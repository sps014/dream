//! Candidate ownership tokens and their local forwarding aliases.

use super::*;

/// Owned RC locals whose every definition is a load through the same base and slot followed by
/// its own `Retain`, with no other retain and no hand-off of the count by it or an alias.
pub(super) fn candidates(
    f: &MirFunction,
    interner: &TypeInterner,
    layouts: &LayoutTable,
) -> BTreeMap<u32, Candidate> {
    let params: IndexSet<u32> = f.params.iter().map(|p| p.0).collect();
    let eligible = |x: u32| {
        let d = &f.locals[x as usize];
        !params.contains(&x)
            && !d.is_cursor
            && interner.is_reference(d.ty)
            && !interner.is_shared_type(d.ty)
    };
    let mut snaps: IndexMap<u32, Snapshot> = IndexMap::new();
    let mut bad_snapshot: IndexSet<u32> = IndexSet::new();
    let mut given_away: IndexSet<u32> = IndexSet::new();
    let mut defs: IndexMap<u32, usize> = IndexMap::new();
    let mut rc_ops: IndexMap<u32, (usize, usize)> = IndexMap::new();
    // Local-to-local copies `dest = src` not followed by a retain of `dest`.
    let mut alias_defs: IndexMap<u32, Vec<u32>> = IndexMap::new();
    // `dest = move src` between locals: the count changes hands within the frame, so it only
    // escapes when `dest` ends up outside the snapshot's class.
    let mut local_moves: Vec<(u32, u32)> = Vec::new();
    let mut other_defs: IndexSet<u32> = IndexSet::new();
    let mut nonnull_defs = params.clone();
    for block in &f.blocks {
        for (si, stmt) in block.stmts.iter().enumerate() {
            match stmt {
                Statement::Assign(Place::Local(d), rv)
                    if !matches!(rv, Rvalue::Use(Operand::Const(Const::Null))) =>
                {
                    nonnull_defs.insert(d.0);
                    let retained = retained_after(&block.stmts[si + 1..], d.0);
                    // `d = s; ... s = null` with no release of `s` hands `s`'s count to `d`
                    // even when `d` later retains again for some other use.
                    if let Rvalue::Use(op) | Rvalue::Cast(op, _, _) = rv
                        && let Some(s) = local_of(op).filter(|&s| s != d.0)
                        && retained
                        && nulled_after(&block.stmts[si + 1..], s)
                    {
                        local_moves.push((d.0, s));
                    }
                    match rv {
                        Rvalue::Use(op) | Rvalue::Cast(op, _, _)
                            if local_of(op).is_some_and(|s| s != d.0) && !retained =>
                        {
                            alias_defs
                                .entry(d.0)
                                .or_default()
                                .push(local_of(op).expect("some"));
                        }
                        Rvalue::Move { src, .. } if src.0 != d.0 && !retained => {
                            alias_defs.entry(d.0).or_default().push(src.0);
                            local_moves.push((d.0, src.0));
                        }
                        Rvalue::UnionField { base, .. }
                            if local_of(base).is_some_and(|s| {
                                interner.is_niche_union(f.locals[s as usize].ty)
                            }) && !retained =>
                        {
                            alias_defs
                                .entry(d.0)
                                .or_default()
                                .push(local_of(base).expect("local"));
                        }
                        _ => {
                            other_defs.insert(d.0);
                        }
                    }
                    if eligible(d.0) {
                        *defs.entry(d.0).or_default() += 1;
                        let ok = retained
                            && load_of(f, rv, layouts).is_some_and(|s| {
                                s.base != d.0 && *snaps.entry(d.0).or_insert(s) == s
                            });
                        if !ok {
                            bad_snapshot.insert(d.0);
                        }
                    }
                }
                Statement::Retain(Operand::Copy(Place::Local(l))) => {
                    rc_ops.entry(l.0).or_default().0 += 1;
                }
                Statement::Release(Operand::Copy(Place::Local(l))) => {
                    rc_ops.entry(l.0).or_default().1 += 1;
                }
                _ => {}
            }
            let alias_move = matches!(stmt, Statement::Assign(Place::Local(d), Rvalue::Move { src, .. })
                if local_moves.last() == Some(&(d.0, src.0)));
            if !alias_move {
                for_each_given_away(stmt, |l| {
                    given_away.insert(l);
                });
            }
        }
        if let Some(l) = terminator_hands_on(&block.terminator) {
            given_away.insert(l);
        }
    }
    let mut out = BTreeMap::new();
    for (x, snap) in snaps {
        if bad_snapshot.contains(&x) || rc_ops.get(&x).map(|r| r.0) != defs.get(&x).copied() {
            continue;
        }
        // Copies of a parameter are interchangeable, so after copy propagation a retain of one
        // copy can be the count another copy returns or sinks (`_c = p; retain _c; return p`).
        if snap.slot == Slot::Param && param_handed_on(f, snap.base, &given_away) {
            continue;
        }
        let mut class: BTreeSet<u32> = BTreeSet::from([x]);
        let mut ok = true;
        let mut grew = true;
        while grew && ok {
            grew = false;
            for (&y, srcs) in &alias_defs {
                if class.contains(&y) || !srcs.iter().any(|s| class.contains(s)) {
                    continue;
                }
                // An alias that retained for itself holds a second count. A release with no
                // retain is the snapshot's count, moved into the alias (`op = elem`).
                let own_retain = rc_ops.get(&y).is_some_and(|r| r.0 != 0);
                if params.contains(&y)
                    || other_defs.contains(&y)
                    || own_retain
                    || !srcs
                        .iter()
                        .all(|s| class.contains(s) || !nonnull_defs.contains(s))
                {
                    ok = false;
                    break;
                }
                class.insert(y);
                grew = true;
            }
        }
        let moved_out = local_moves
            .iter()
            .any(|(d, s)| class.contains(s) && !class.contains(d));
        if ok && !moved_out && !class.iter().any(|m| given_away.contains(m)) {
            out.insert(x, Candidate { snap, class });
        }
    }
    out
}

/// Whether `p` or any local copied from it (transitively) gives a count away.
fn param_handed_on(f: &MirFunction, p: u32, given_away: &IndexSet<u32>) -> bool {
    let mut copies: IndexSet<u32> = IndexSet::from([p]);
    let mut grew = true;
    while grew {
        grew = false;
        for block in &f.blocks {
            for stmt in &block.stmts {
                let Statement::Assign(Place::Local(d), rv) = stmt else {
                    continue;
                };
                let src = match rv {
                    Rvalue::Use(op) | Rvalue::Cast(op, _, _) => local_of(op),
                    Rvalue::Move { src, .. } => Some(src.0),
                    _ => None,
                };
                if src.is_some_and(|s| copies.contains(&s)) && copies.insert(d.0) {
                    grew = true;
                }
            }
        }
    }
    copies.iter().any(|c| given_away.contains(c))
}

fn load_of(f: &MirFunction, rv: &Rvalue, layouts: &LayoutTable) -> Option<Snapshot> {
    load_of_inner(f, rv, layouts, &mut IndexSet::new())
}

fn load_of_inner(
    f: &MirFunction,
    rv: &Rvalue,
    layouts: &LayoutTable,
    seen: &mut IndexSet<u32>,
) -> Option<Snapshot> {
    match rv {
        Rvalue::Use(Operand::Copy(Place::Field { base, field })) => {
            let ty = f.local_ty(*base);
            let fl = layouts.get(ty)?.fields.get(*field)?;
            if fl.is_weak || fl.is_unowned {
                return None;
            }
            if f.locals[base.0 as usize].is_ref {
                let mut array = None;
                for s in f.blocks.iter().flat_map(|b| &b.stmts) {
                    if let Statement::Assign(Place::Local(d), rv) = s
                        && d == base
                    {
                        let Rvalue::Use(Operand::Copy(Place::Index { base: a, .. })) = rv else {
                            return None;
                        };
                        if array.is_some_and(|old| old != a.0) {
                            return None;
                        }
                        array = Some(a.0);
                    }
                }
                let array = array?;
                return Some(Snapshot {
                    base: array,
                    slot: Slot::ElemField(f.locals[array as usize].ty, *field as u32),
                    index: None,
                });
            }
            Some(Snapshot {
                base: base.0,
                slot: Slot::Field(ty, *field as u32),
                index: None,
            })
        }
        Rvalue::Use(Operand::Copy(Place::Index { base, index, .. })) => Some(Snapshot {
            base: base.0,
            slot: Slot::Elem(f.local_ty(*base)),
            index: constant_index(index),
        }),
        Rvalue::UnionField {
            base: Operand::Copy(Place::Local(b)),
            ..
        } => Some(Snapshot {
            base: b.0,
            slot: Slot::Payload,
            index: None,
        }),
        Rvalue::Use(Operand::Copy(Place::Local(p)))
            if !f.locals[p.0 as usize].is_ref && !f.locals[p.0 as usize].is_cursor =>
        {
            Some(Snapshot {
                base: p.0,
                slot: Slot::Param,
                index: None,
            })
        }
        Rvalue::Use(Operand::Copy(Place::Local(p)))
            if f.locals[p.0 as usize].is_cursor && seen.insert(p.0) =>
        {
            let mut snapshot = None;
            for stmt in f.blocks.iter().flat_map(|b| &b.stmts) {
                if let Statement::Assign(Place::Local(d), rv) = stmt
                    && d == p
                    && !matches!(rv, Rvalue::Use(Operand::Const(Const::Null)))
                {
                    let next = load_of_inner(f, rv, layouts, &mut seen.clone())?;
                    // Field and element snapshots can outlive the slot contents they loaded.
                    // Only a caller-pinned parameter survives forwarding without a time proof.
                    if next.slot != Slot::Param {
                        return None;
                    }
                    if snapshot.is_some_and(|prior| prior != next) {
                        return None;
                    }
                    snapshot = Some(next);
                }
            }
            snapshot
        }
        _ => None,
    }
}
