//! A local owner's existing lifetime can cover a view without extending either lifetime.

use super::*;
use crate::analysis::escape::{Escape, LocalEscape, ParamSummaries};
use crate::passes::rc::liveness;
use crate::passes::rc::modref::{Effect, ModRefTable, stmt_effects};

pub(super) fn source_stays_alive(
    f: &MirFunction,
    owner: Local,
    family: &[Option<usize>],
    interner: &TypeInterner,
    layouts: &LayoutTable,
    modref: &ModRefTable,
    panics: &BTreeSet<DefId>,
) -> bool {
    if f.local_ty(owner) != interner.string()
        || f.params.contains(&owner)
        || f.locals[owner.0 as usize].is_cursor
    {
        return false;
    }
    let mut definitions = 0;
    for s in f.blocks.iter().flat_map(|b| &b.stmts) {
        match s {
            Statement::Assign(Place::Local(d), rv) if *d == owner => {
                if !matches!(rv, Rvalue::Use(Operand::Const(Const::Null))) {
                    // Empty concatenation operands can return an existing, published string.
                    // A nonempty literal makes the result fresh or that immortal literal.
                    if !matches!(rv, Rvalue::Concat(args) if args.len() >= 2
                        && args.iter().any(|op| matches!(op,
                            Operand::Const(Const::Str(s)) if !s.is_empty())))
                        && !matches!(rv, Rvalue::ConcatInt { .. })
                    {
                        return false;
                    }
                    definitions += 1;
                }
            }
            // A transferred token no longer protects the source. Local copies can hide such
            // transfers; let the ordinary frame-stable alias proof handle those instead.
            Statement::Assign(Place::Local(_), Rvalue::Use(op)) if local_of(op) == Some(owner) => {
                return false;
            }
            Statement::Assign(_, Rvalue::Move { src, .. }) if *src == owner => return false,
            Statement::Call { callee, args }
            | Statement::Assign(_, Rvalue::Call { callee, args }) => {
                if args.iter().enumerate().any(|(i, op)| {
                    local_of(op) == Some(owner)
                        && callee.take_params.get(i).copied().unwrap_or(false)
                }) {
                    return false;
                }
            }
            Statement::ForceFree(op) if local_of(op) == Some(owner) => return false,
            _ => {}
        }
    }
    if definitions != 1 {
        return false;
    }
    let mut aliases = BTreeSet::from([owner]);
    let mut dependent = BTreeSet::new();
    loop {
        let before = (aliases.len(), dependent.len());
        for s in f.blocks.iter().flat_map(|b| &b.stmts) {
            if let Statement::Assign(place, Rvalue::Use(op)) = s {
                let from_owner = local_of(op).is_some_and(|l| aliases.contains(&l))
                    || matches!(op, Operand::Copy(Place::Field { base, .. })
                        if family[base.0 as usize].is_some_and(|r| dependent.contains(&r)));
                if from_owner {
                    match place {
                        Place::Local(d) if f.local_ty(*d) == interner.string() => {
                            aliases.insert(*d);
                        }
                        Place::Field { base, .. } => {
                            if let Some(r) = family[base.0 as usize] {
                                dependent.insert(r);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        if before == (aliases.len(), dependent.len()) {
            break;
        }
    }
    if dependent.is_empty() {
        return false;
    }
    for s in f.blocks.iter().flat_map(|b| &b.stmts) {
        match s {
            Statement::Assign(_, Rvalue::Move { src, .. }) if aliases.contains(src) => {
                return false;
            }
            Statement::Call { callee, args }
            | Statement::Assign(_, Rvalue::Call { callee, args })
                if args.iter().enumerate().any(|(i, op)| {
                    local_of(op).is_some_and(|l| aliases.contains(&l))
                        && callee.take_params.get(i).copied().unwrap_or(false)
                }) =>
            {
                return false;
            }
            _ => {}
        }
    }
    let member = |l: Local| family[l.0 as usize].is_some_and(|r| dependent.contains(&r));
    if !views_stay_private(f, &member) {
        return false;
    }
    let dependent_read = |l: Local| member(l) || (l != owner && aliases.contains(&l));
    let mut reads = MirFunction {
        batched_construction: f.batched_construction,
        def: f.def,
        instance: f.instance.clone(),
        name: String::new(),
        symbol: f.symbol.clone(),
        params: f.params.clone(),
        ret: f.ret,
        locals: f.locals.clone(),
        blocks: f.blocks.clone(),
        entry: f.entry,
        is_async: f.is_async,
        hir_fn: None,
        file: None,
        inline: dream_hir::InlineHint::Default,
    };
    for b in &mut reads.blocks {
        b.stmts.retain(|s| {
            !matches!(s,
            Statement::ValueRetain(l) | Statement::ValueDrop(l) | Statement::ValueKill(l)
                if member(*l))
        });
    }
    let live_out = liveness::live_out(&reads);
    let mut private = reads.blocks.clone();
    for b in &mut reads.blocks {
        for s in &mut b.stmts {
            if matches!(s, Statement::Assign(Place::Field { base, .. }, _) if member(*base)) {
                *s = Statement::Nop;
            }
            stmt_operands_mut(s, &mut |op| {
                if let Operand::Copy(Place::Field { base, field }) = op
                    && member(*base)
                    && layouts
                        .get(f.local_ty(*base))
                        .is_some_and(|l| interner.is_reference(l.fields[*field].ty))
                {
                    *op = Operand::Copy(Place::Local(owner));
                }
            });
        }
        terminator_operands_mut(&mut b.terminator, &mut |op| {
            if let Operand::Copy(Place::Field { base, field }) = op
                && member(*base)
                && layouts
                    .get(f.local_ty(*base))
                    .is_some_and(|l| interner.is_reference(l.fields[*field].ty))
            {
                *op = Operand::Copy(Place::Local(owner));
            }
        });
    }
    let escape = LocalEscape::analyze(&reads, interner, &ParamSummaries::default());
    let private_source = aliases.iter().all(|&l| escape.of(l) == Escape::No)
        && (0..family.len())
            .all(|i| !member(Local(i as u32)) || escape.of(Local(i as u32)) == Escape::No);
    std::mem::swap(&mut reads.blocks, &mut private);
    if !private_source {
        return false;
    }
    let in_flight: Vec<Vec<bool>> = reads
        .blocks
        .iter()
        .enumerate()
        .map(|(bi, b)| {
            let mut live = live_out[bi].clone();
            liveness::add_terminator_reads(&b.terminator, &mut live);
            let mut points = vec![false; b.stmts.len()];
            for (si, s) in b.stmts.iter().enumerate().rev() {
                points[si] = live.iter().any(|&l| dependent_read(Local(l)));
                liveness::transfer_stmt(s, &mut live);
            }
            points
        })
        .collect();
    // Possible states: uninitialized, alive, released. Union at joins makes a read after
    // release on even one predecessor fail, including back edges and partial initialization.
    let mut incoming = vec![0u8; f.blocks.len()];
    incoming[f.entry.0 as usize] = 1;
    let step = |state: u8, s: &Statement| match s {
        Statement::Assign(Place::Local(d), rv) if *d == owner => {
            if matches!(rv, Rvalue::Use(Operand::Const(Const::Null))) {
                4
            } else {
                2
            }
        }
        Statement::Release(op) if local_of(op) == Some(owner) => 4,
        _ => state,
    };
    loop {
        let mut changed = false;
        for (bi, b) in f.blocks.iter().enumerate() {
            if incoming[bi] == 0 {
                continue;
            }
            let state = b.stmts.iter().fold(incoming[bi], step);
            for next in b.terminator.successors() {
                let old = incoming[next.0 as usize];
                incoming[next.0 as usize] |= state;
                changed |= old != incoming[next.0 as usize];
            }
        }
        if !changed {
            break;
        }
    }
    for (bi, b) in reads.blocks.iter().enumerate() {
        if incoming[bi] == 0 {
            continue;
        }
        let mut state = incoming[bi];
        for (si, s) in b.stmts.iter().enumerate() {
            let mut uses = false;
            stmt_reads(s, &mut |l| uses |= dependent_read(l));
            if uses && state != 2 {
                return false;
            }
            if uses || in_flight[bi][si] {
                // A back edge can execute the same definition again. An alive new token
                // cannot protect a view of the previous allocation after release/replacement.
                if matches!(s, Statement::Release(op) if local_of(op) == Some(owner))
                    || (state & 2 != 0
                        && matches!(s, Statement::Assign(Place::Local(d), _) if *d == owner))
                {
                    return false;
                }
                let mut quiet = true;
                stmt_effects(s, f, interner, |e| {
                    quiet &= match e {
                        Effect::Top => false,
                        // Panic hooks cannot access this fresh, unpublished string or its views.
                        // The normal owner remains alive, and panic never returns to cleanup.
                        Effect::Call(c) => panics.contains(&c.def) || modref.call(c).is_quiet(),
                        Effect::Iface(id, slot) => modref.iface(id, slot).is_quiet(),
                        Effect::Ctor(def) => modref.ctor(def).is_quiet(),
                        _ => true,
                    }
                });
                if let Statement::Release(op) = s {
                    quiet &= local_of(op)
                        .is_some_and(|l| !modref.may_run_del(f.local_ty(l), interner, layouts));
                }
                if let Statement::ValueDrop(l) = s {
                    quiet &= !modref.may_run_del(f.local_ty(*l), interner, layouts);
                }
                quiet &= !matches!(s, Statement::DeferLeave(_) | Statement::RegionLeave);
                if !quiet {
                    return false;
                }
            }
            state = step(state, s);
        }
        let mut uses = false;
        terminator_reads(&b.terminator, &mut |l| uses |= dependent_read(l));
        if uses && state != 2 {
            return false;
        }
    }
    true
}

fn views_stay_private(f: &MirFunction, member: &impl Fn(Local) -> bool) -> bool {
    for b in &f.blocks {
        for s in &b.stmts {
            match s {
                Statement::Assign(
                    Place::Local(d),
                    Rvalue::Use(Operand::Copy(Place::Local(src))),
                ) if member(*d) && member(*src) => continue,
                Statement::ValueRetain(l) | Statement::ValueDrop(l) | Statement::ValueKill(l)
                    if member(*l) =>
                {
                    continue;
                }
                _ => {}
            }
            let mut s = s.clone();
            if let Statement::Assign(Place::Field { base, .. }, rv) = &s
                && member(*base)
            {
                s = Statement::Assign(Place::Local(Local(u32::MAX)), rv.clone());
            }
            stmt_operands_mut(&mut s, &mut |op| {
                if matches!(op, Operand::Copy(Place::Field { base, .. }) if member(*base)) {
                    *op = Operand::Const(Const::Null);
                }
            });
            let mut escapes = false;
            stmt_reads(&s, &mut |l| escapes |= member(l));
            if escapes {
                return false;
            }
        }
        let mut t = b.terminator.clone();
        terminator_operands_mut(&mut t, &mut |op| {
            if matches!(op, Operand::Copy(Place::Field { base, .. }) if member(*base)) {
                *op = Operand::Const(Const::Null);
            }
        });
        let mut escapes = false;
        terminator_reads(&t, &mut |l| escapes |= member(l));
        if escapes {
            return false;
        }
    }
    true
}
