//! Nested-reference ownership of value structs: copies, sink transfers, and last-use drops.

use super::liveness::{self, live_after_stmt, stmt_reads_local};
use super::tokens::{assigns_local, sink_call_args, source_line_end};
use crate::{Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;
use indexmap::IndexMap;
use std::collections::{BTreeMap, BTreeSet};

/// Last-use move for glue value structs: still-live copies/args get [`Statement::ValueRetain`];
/// last-use transfers get [`Statement::ValueKill`] (callee / dest inherits nested refs).
pub(super) fn insert_value_struct_moves(
    func: &mut MirFunction,
    interner: &TypeInterner,
    changed: &mut bool,
) {
    let is_value_src = |idx: usize| {
        let d = &func.locals[idx];
        interner.is_value_type(d.ty)
            && !d.is_ref
            && (d.name.is_some() || defined_only_by_calls(func, idx as u32))
            && d.name.as_deref() != Some("this")
    };
    let live_out = liveness::live_out(func);
    let mut retain_before: Vec<(usize, usize, u32, u32)> = Vec::new();
    let mut retain_after: Vec<(usize, usize, u32)> = Vec::new();
    let mut kill_after: Vec<(usize, usize, u32)> = Vec::new();
    for (bi, block) in func.blocks.iter().enumerate() {
        for (si, stmt) in block.stmts.iter().enumerate() {
            if matches!(
                stmt,
                Statement::ValueRetain(_) | Statement::ValueKill(_) | Statement::ValueDrop(_)
            ) {
                continue;
            }
            if let Statement::Assign(
                Place::Local(dest),
                Rvalue::Use(Operand::Copy(Place::Local(src))),
            ) = stmt
            {
                if dest.0 != src.0
                    && is_value_src(src.0 as usize)
                    && func.locals[dest.0 as usize].name.is_some()
                    && !func.locals[dest.0 as usize].is_ref
                    && interner.is_value_type(func.locals[dest.0 as usize].ty)
                {
                    if live_after_stmt(func, &live_out, bi, si, src.0) {
                        retain_after.push((bi, si, dest.0));
                    } else {
                        kill_after.push((bi, si, src.0));
                    }
                }
            }
            let mut counts: std::collections::BTreeMap<u32, u32> =
                std::collections::BTreeMap::new();
            for local in value_arg_locals(func, stmt, interner, &is_value_src) {
                *counts.entry(local).or_insert(0) += 1;
            }
            for (local, n) in counts {
                let last_use = !live_after_stmt(func, &live_out, bi, si, local);
                let retains = if last_use { n.saturating_sub(1) } else { n };
                if retains > 0 {
                    retain_before.push((bi, si, local, retains));
                }
                if last_use {
                    kill_after.push((bi, si, local));
                }
            }
        }
    }
    if retain_before.is_empty() && retain_after.is_empty() && kill_after.is_empty() {
        return;
    }
    let mut before_by: IndexMap<usize, Vec<(usize, u32, u32)>> = IndexMap::new();
    let mut after_by: IndexMap<usize, Vec<(usize, u32)>> = IndexMap::new();
    let mut kill_by: IndexMap<usize, Vec<(usize, u32)>> = IndexMap::new();
    for (bi, si, local, n) in retain_before {
        before_by.entry(bi).or_default().push((si, local, n));
    }
    for (bi, si, local) in retain_after {
        after_by.entry(bi).or_default().push((si, local));
    }
    let mut newly_killed: BTreeSet<u32> = BTreeSet::new();
    for (bi, si, local) in kill_after {
        if !func.locals[local as usize].manual_drop {
            newly_killed.insert(local);
        }
        func.locals[local as usize].manual_drop = true;
        kill_by.entry(bi).or_default().push((si, local));
    }
    insert_killed_value_exit_drops(func, &newly_killed, changed);
    let mut blocks: Vec<usize> = before_by
        .keys()
        .chain(after_by.keys())
        .chain(kill_by.keys())
        .copied()
        .collect();
    blocks.sort_unstable();
    blocks.dedup();
    for bi in blocks {
        let before = before_by.swap_remove(&bi).unwrap_or_default();
        let after = after_by.swap_remove(&bi).unwrap_or_default();
        let kills = kill_by.swap_remove(&bi).unwrap_or_default();
        let mut out: Vec<Statement> = Vec::with_capacity(func.blocks[bi].stmts.len() + 4);
        for (si, stmt) in func.blocks[bi].stmts.drain(..).enumerate() {
            for (rsi, local, n) in &before {
                if *rsi == si {
                    for _ in 0..*n {
                        out.push(Statement::ValueRetain(Local(*local)));
                    }
                    *changed = true;
                }
            }
            out.push(stmt);
            for (asi, local) in &after {
                if *asi == si {
                    out.push(Statement::ValueRetain(Local(*local)));
                    *changed = true;
                }
            }
            for (ksi, local) in &kills {
                if *ksi == si {
                    out.push(Statement::ValueKill(Local(*local)));
                    *changed = true;
                }
            }
        }
        func.blocks[bi].stmts = out;
    }
}

/// A kill hands the value's nested refs away on *one* path, but `manual_drop` opts the local out
/// of frame teardown on every path. `ValueKill` zeroes the storage, so dropping at each `Return`
/// is a no-op where it was moved and releases it where it was not.
fn insert_killed_value_exit_drops(
    func: &mut MirFunction,
    killed: &BTreeSet<u32>,
    changed: &mut bool,
) {
    if killed.is_empty() {
        return;
    }
    for block in &mut func.blocks {
        let Terminator::Return(ret) = &block.terminator else {
            continue;
        };
        let returned = match ret {
            Some(Operand::Copy(Place::Local(l))) => Some(l.0),
            _ => None,
        };
        for &local in killed {
            if Some(local) != returned {
                block.stmts.push(Statement::ValueDrop(Local(local)));
                *changed = true;
            }
        }
    }
}

fn value_arg_locals(
    func: &MirFunction,
    stmt: &Statement,
    interner: &TypeInterner,
    is_value_src: &dyn Fn(usize) -> bool,
) -> Vec<u32> {
    let (take_params, args) = match (sink_call_args(stmt), stmt) {
        (Some(sink), _) => sink,
        (None, Statement::Assign(_, Rvalue::UnionNew { args, .. })) => {
            (vec![true; args.len()], args.as_slice())
        }
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if !take_params.get(i).copied().unwrap_or(false) {
            continue;
        }
        if let Operand::Copy(Place::Local(l)) = arg {
            let root = value_copy_root(func, l.0);
            if is_value_src(root as usize) && interner.is_value_type(func.locals[root as usize].ty)
            {
                out.push(root);
            }
        }
    }
    out
}

/// Unnamed value temps that only copy a local are aliases of that local (emitter Borrow).
fn value_copy_root(func: &MirFunction, local: u32) -> u32 {
    let decl = &func.locals[local as usize];
    if decl.name.is_some() || decl.is_ref {
        return local;
    }
    let mut src = None;
    let mut defs = 0u32;
    for block in &func.blocks {
        for stmt in &block.stmts {
            if let Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Copy(Place::Local(s)))) =
                stmt
            {
                if d.0 == local {
                    defs += 1;
                    src = Some(s.0);
                }
            } else if let Statement::Assign(Place::Local(d), _) = stmt {
                if d.0 == local {
                    defs += 1;
                    src = None;
                }
            }
        }
    }
    if defs == 1 {
        if let Some(s) = src {
            return value_copy_root(func, s);
        }
    }
    local
}

/// Returning a value local transfers nested refs via sret blit; skip frame-exit drop.
pub(super) fn mark_returned_value_locals_moved(
    func: &mut MirFunction,
    interner: &TypeInterner,
    changed: &mut bool,
) {
    if !interner.is_value_type(func.ret) {
        return;
    }
    for block in &func.blocks {
        if let Terminator::Return(Some(Operand::Copy(Place::Local(l))))
        | Terminator::AsyncComplete(Some(Operand::Copy(Place::Local(l)))) = &block.terminator
        {
            if interner.is_value_type(func.locals[l.0 as usize].ty)
                && !func.locals[l.0 as usize].is_ref
                && !func.locals[l.0 as usize].manual_drop
            {
                func.locals[l.0 as usize].manual_drop = true;
                *changed = true;
            }
        }
    }
}

fn is_owning_value_local(func: &MirFunction, interner: &TypeInterner, idx: usize) -> bool {
    let decl = &func.locals[idx];
    if !interner.is_value_type(decl.ty) || decl.is_ref || decl.manual_drop {
        return false;
    }
    if decl.name.as_deref() == Some("this") {
        return false;
    }
    if idx < func.params.len() {
        return false;
    }
    decl.name.is_some() || defined_only_by_calls(func, idx as u32)
}

/// An unnamed temp holding a call result owns it; once the call is inlined the temp is defined by
/// a plain copy, which frame teardown treats as an alias, so its drop must be placed here.
fn defined_only_by_calls(func: &MirFunction, local: u32) -> bool {
    let mut seen = false;
    for stmt in func.blocks.iter().flat_map(|b| &b.stmts) {
        if let Statement::Assign(Place::Local(d), rv) = stmt {
            if d.0 != local {
                continue;
            }
            if !matches!(
                rv,
                Rvalue::Call { .. } | Rvalue::IndirectCall { .. } | Rvalue::InterfaceCall { .. }
            ) {
                return false;
            }
            seen = true;
        } else if assigns_local(stmt, local) {
            return false;
        }
    }
    seen
}

/// Early `ValueDrop` after the last use of an owning value local. Whole-value copy-out (`dest = src`)
/// keeps copy semantics (frame teardown still drops `src`); last *read* drops immediately.
pub(super) fn insert_complete_value_drops(
    func: &mut MirFunction,
    interner: &TypeInterner,
    bi: usize,
    skip: Option<u32>,
    changed: &mut bool,
) {
    let n = func.locals.len();
    for i in 0..n {
        if skip == Some(i as u32) || !is_owning_value_local(func, interner, i) {
            continue;
        }
        let already = func.blocks[bi]
            .stmts
            .iter()
            .any(|s| matches!(s, Statement::ValueDrop(l) if l.0 == i as u32));
        if already {
            continue;
        }
        func.locals[i].manual_drop = true;
        func.blocks[bi]
            .stmts
            .push(Statement::ValueDrop(Local(i as u32)));
        *changed = true;
    }
}

/// Early `ValueDrop` after the last use of an owning value local. Whole-value copy-out (`dest = src`)
/// keeps copy semantics (frame teardown still drops `src`); last *read* drops immediately.
/// `skip_await_blocks` holds nested refs until resume/`AsyncComplete` (host may stash pointers).
pub(super) fn insert_early_value_drops(
    func: &mut MirFunction,
    interner: &TypeInterner,
    changed: &mut bool,
    skip_await_blocks: bool,
) {
    let live_out = liveness::live_out(func);
    let owning: Vec<u32> = (0..func.locals.len())
        .filter(|&i| is_owning_value_local(func, interner, i))
        .map(|i| i as u32)
        .collect();
    if owning.is_empty() {
        return;
    }
    let borrowers = payload_borrowers(func, &owning);
    let mut drop_at: Vec<(usize, usize, u32)> = Vec::new();
    for (bi, block) in func.blocks.iter().enumerate() {
        if skip_await_blocks && matches!(block.terminator, Terminator::Await { .. }) {
            continue;
        }
        for (si, stmt) in block.stmts.iter().enumerate() {
            for &local in &owning {
                let deps = borrowers.get(&local).map_or(&[][..], |v| v.as_slice());
                if deps
                    .iter()
                    .any(|&c| live_after_stmt(func, &live_out, bi, si, c))
                {
                    continue;
                }
                if matches!(
                    stmt,
                    Statement::ValueDrop(l) | Statement::ValueKill(l) | Statement::ValueRetain(l)
                        if l.0 == local
                ) {
                    continue;
                }
                let whole_copy_out = matches!(
                    stmt,
                    Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Copy(Place::Local(s))))
                        if s.0 == local && d.0 != local
                );
                if whole_copy_out {
                    continue;
                }
                let used = stmt_reads_local(stmt, local)
                    || assigns_local(stmt, local)
                    || deps.iter().any(|&c| stmt_reads_local(stmt, c));
                if !used || live_after_stmt(func, &live_out, bi, si, local) {
                    continue;
                }
                drop_at.push((bi, source_line_end(block, si), local));
            }
        }
    }
    if drop_at.is_empty() {
        return;
    }
    let mut by_block: IndexMap<usize, Vec<(usize, u32)>> = IndexMap::new();
    for (bi, si, local) in drop_at {
        by_block.entry(bi).or_default().push((si, local));
        func.locals[local as usize].manual_drop = true;
    }
    for (bi, sites) in by_block {
        let mut out: Vec<Statement> = Vec::with_capacity(func.blocks[bi].stmts.len() + sites.len());
        for (si, stmt) in func.blocks[bi].stmts.drain(..).enumerate() {
            out.push(stmt);
            for (ssi, local) in &sites {
                if *ssi == si {
                    out.push(Statement::ValueDrop(Local(*local)));
                    *changed = true;
                }
            }
        }
        func.blocks[bi].stmts = out;
    }
}

/// Cursor locals bound (unretained) to a reference inside an owning value local — a union payload
/// or a struct field. The value local owns that reference, so it must outlive every borrower.
fn payload_borrowers(func: &MirFunction, owning: &[u32]) -> BTreeMap<u32, Vec<u32>> {
    let mut out: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            let Statement::Assign(Place::Local(d), rv) = stmt else {
                continue;
            };
            if !func.locals[d.0 as usize].is_cursor {
                continue;
            }
            let base = match rv {
                Rvalue::UnionField {
                    base: Operand::Copy(Place::Local(b)),
                    ..
                } => *b,
                Rvalue::Use(Operand::Copy(Place::Field { base, .. })) => *base,
                _ => continue,
            };
            if owning.contains(&base.0) {
                let deps = out.entry(base.0).or_default();
                if !deps.contains(&d.0) {
                    deps.push(d.0);
                }
            }
        }
    }
    out
}
