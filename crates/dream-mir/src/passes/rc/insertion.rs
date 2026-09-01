//! [`RcInsertion`]: make reference ownership explicit in MIR via compile-time tokens.

use super::is_borrowed_copy;
use super::liveness::{self, live_after_stmt, live_in_of, stmt_reads_local};
use super::tokens::{
    apply_stmt_tokens, assigns_local, dest_holds_token, is_owned_local, move_source,
    needs_rebind_temp, rc_op_on_local, release_and_null, sink_call_args, source_line_end,
    take_arg_effects, TokenAnalysis,
};
use super::uniqueness::{
    apply_stmt_unique, can_unique_destroy, constructed_payload_locals, container_move_locals,
};
use crate::passes::MirPass;
use crate::{Const, Global, Local, LocalDecl, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;
use std::collections::HashMap;

pub struct RcInsertion;

/// Coarse container-slot identity: field slots by `(base, field)`, index slots by base alone
/// (dynamic indices are indistinguishable statically). Used only where a liveness guard makes
/// over-matching safe.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum SlotId {
    Local(u32),
    Global(Global),
    Field(u32, u32),
    IndexBase(u32),
}

fn slot_id(place: &Place) -> SlotId {
    match place {
        Place::Local(l) => SlotId::Local(l.0),
        Place::Global(g) => SlotId::Global(*g),
        Place::Field { base, field } => SlotId::Field(base.0, *field as u32),
        Place::Index { base, .. } => SlotId::IndexBase(base.0),
        Place::Deref { ptr, .. } => SlotId::Local(ptr.0),
    }
}

impl RcInsertion {
    pub(crate) fn run_with_layouts(
        func: &mut MirFunction,
        interner: &TypeInterner,
        layouts: &dream_hir::LayoutTable,
    ) -> bool {
        RcInsertion.run_inner(func, interner, layouts)
    }

    fn run_inner(
        &self,
        func: &mut MirFunction,
        interner: &TypeInterner,
        layouts: &dream_hir::LayoutTable,
    ) -> bool {
        super::cursor::infer_cursors(func, interner);

        let local_is_ref: Vec<bool> = func
            .locals
            .iter()
            .map(|d| interner.is_rc_tracked(d.ty))
            .collect();
        let analysis = TokenAnalysis::analyze(func, interner, layouts);
        let owned_flags: Vec<bool> = (0..func.locals.len() as u32)
            .map(|l| is_owned_local(func, interner, l))
            .collect();
        let is_owned = |l: u32| owned_flags.get(l as usize).copied().unwrap_or(false);
        let mut changed = false;

        let mut realloc_readers: HashMap<(usize, usize), Vec<u32>> = HashMap::new();
        let mut slot_readers: HashMap<SlotId, Vec<u32>> = HashMap::new();
        let live_out_rc = liveness::live_out(func);
        for (bi, block) in func.blocks.iter().enumerate() {
            for (si, stmt) in block.stmts.iter().enumerate() {
                // Collect locals defined by a direct container read, keyed by source slot. A
                // self-realloc of that slot (`f = Buffer.realloc(f, ..)`) consumes the old block
                // outright, so any token still held by such a reader must be dropped *before*
                // the store (see main loop).
                if let Statement::Assign(Place::Local(dest), rv) = stmt {
                    let read_place = match rv {
                        Rvalue::Use(Operand::Copy(p)) | Rvalue::Cast(Operand::Copy(p), _, _) => {
                            Some(p)
                        }
                        _ => None,
                    };
                    if let Some(Place::Field { .. } | Place::Index { .. }) = read_place {
                        if interner.is_rc_tracked(func.locals[dest.0 as usize].ty) {
                            slot_readers
                                .entry(slot_id(read_place.unwrap()))
                                .or_default()
                                .push(dest.0);
                        }
                    }
                }
                let Statement::Assign(dest_place, Rvalue::ArrayRealloc { array, .. }) = stmt
                else {
                    continue;
                };
                let Operand::Copy(src_place) = array else {
                    continue;
                };
                if slot_id(dest_place) != slot_id(src_place) {
                    continue;
                }
                let Some(readers) = slot_readers.get(&slot_id(src_place)) else {
                    continue;
                };
                let ok: Vec<u32> = readers
                    .iter()
                    .copied()
                    .filter(|&x| is_owned(x) && !live_after_stmt(func, &live_out_rc, bi, si, x))
                    .collect();
                if !ok.is_empty() {
                    realloc_readers.insert((bi, si), ok);
                }
            }
        }

        let local_types: Vec<dream_types::TypeId> = func.locals.iter().map(|d| d.ty).collect();
        let take_flags: Vec<bool> = func.locals.iter().map(|d| d.is_take).collect();
        let mut extra_locals: Vec<LocalDecl> = Vec::new();

        let temp_base = func.locals.len() as u32;
        for (bi, block) in func.blocks.iter_mut().enumerate() {
            let mut tokens = analysis.token_in[bi].clone();
            let mut unique = analysis.unique_in[bi].clone();
            let mut out: Vec<Statement> = Vec::with_capacity(block.stmts.len() + 8);
            for &local in &analysis.start_release[bi] {
                // Join/loop-header leftover: the other pred may have copied this pointer into a
                // still-live container. Unique destroy ignores RC and would free that copy.
                out.extend(release_and_null(local, false));
                if (local as usize) < tokens.len() {
                    tokens[local as usize] = false;
                    unique[local as usize] = false;
                }
                changed = true;
            }
            for (si, stmt) in block.stmts.drain(..).enumerate() {
                let ref_dest = match &stmt {
                    Statement::Assign(Place::Local(dest), rvalue) if is_owned(dest.0) => Some((
                        *dest,
                        is_borrowed_copy(rvalue, interner),
                        needs_rebind_temp(rvalue, dest.0),
                        move_source(rvalue, &is_owned),
                    )),
                    _ => None,
                };
                let had_dest = ref_dest
                    .as_ref()
                    .map(|(d, _, _, _)| dest_holds_token(&tokens, d.0))
                    .unwrap_or(false);
                let had_unique = ref_dest
                    .as_ref()
                    .map(|(d, _, _, _)| {
                        unique.get(d.0 as usize).copied().unwrap_or(false)
                            && !constructed_payload_locals(&stmt).contains(&d.0)
                    })
                    .unwrap_or(false);
                let container_srcs = container_move_locals(&stmt);
                // Self-realloc of a slot destroys the block under any read-derived owner of it
                // (the lowering emits `$realloc` with no release-old step). Release those owners
                // first: dead ones restore the slot's uniqueness so the move is legitimate.
                // Liveness-guarded: a reader still used later is left untouched (such code reads
                // freed memory under any scheme short of copy-on-realloc).
                let mut pre_releases: Vec<Statement> = Vec::new();
                if let Some(readers) = realloc_readers.get(&(bi, si)) {
                    for &x in readers {
                        if !dest_holds_token(&tokens, x) {
                            continue;
                        }
                        pre_releases.extend(release_and_null(x, false));
                    }
                }
                for r in &pre_releases {
                    if let Statement::Release(Operand::Copy(Place::Local(l))) = r {
                        tokens[l.0 as usize] = false;
                    } else if let Statement::ReleaseUnique(Operand::Copy(Place::Local(l))) = r {
                        tokens[l.0 as usize] = false;
                    }
                    changed = true;
                }
                out.extend(pre_releases);
                apply_stmt_tokens(
                    &stmt,
                    interner,
                    &is_owned,
                    analysis.assign_move.contains(&(bi, si)),
                    |l| analysis.sink_move.contains(&(bi, si, l)),
                    &mut tokens,
                );
                apply_stmt_unique(
                    &stmt,
                    interner,
                    &is_owned,
                    analysis.assign_move.contains(&(bi, si)),
                    |l| analysis.sink_move.contains(&(bi, si, l)),
                    &mut unique,
                );

                let (sink_retains, sink_nulls) =
                    take_arg_effects(&stmt, &is_owned, &local_is_ref, |local| {
                        analysis.sink_move.contains(&(bi, si, local))
                    });

                match ref_dest {
                    Some((dest, retain, true, _)) if had_dest => {
                        let tmp = Local(temp_base + extra_locals.len() as u32);
                        extra_locals.push(LocalDecl {
                            ty: local_types[dest.0 as usize],
                            name: None,
                            is_ref: false,
                            is_take: false,
                            is_cursor: false,
                            manual_drop: false,
                        });
                        let rvalue = match stmt {
                            Statement::Assign(_, rv) => rv,
                            _ => unreachable!("ref_dest is an Assign"),
                        };
                        for r in sink_retains {
                            out.push(r);
                        }
                        out.push(Statement::Assign(Place::Local(tmp), rvalue));
                        out.push(release_one(
                            dest.0,
                            unique_destroy(interner, &local_types, &take_flags, dest.0, had_unique),
                        ));
                        out.push(Statement::Assign(
                            Place::Local(dest),
                            Rvalue::Use(Operand::Copy(Place::Local(tmp))),
                        ));
                        if retain {
                            out.push(Statement::Retain(Operand::Copy(Place::Local(dest))));
                        }
                        for n in sink_nulls.into_iter().filter(|n| {
                            !matches!(
                                n,
                                Statement::Assign(Place::Local(l), _) if *l == dest
                            )
                        }) {
                            out.push(n);
                        }
                        for src in container_srcs {
                            if analysis.sink_move.contains(&(bi, si, src)) && src != dest.0 {
                                out.push(Statement::Assign(
                                    Place::Local(Local(src)),
                                    Rvalue::Use(Operand::Const(Const::Null)),
                                ));
                            }
                        }
                        changed = true;
                    }
                    Some((dest, retain, true, _)) => {
                        for r in sink_retains {
                            out.push(r);
                        }
                        out.push(stmt);
                        if retain {
                            out.push(Statement::Retain(Operand::Copy(Place::Local(dest))));
                        }
                        for n in sink_nulls {
                            out.push(n);
                        }
                        for src in container_srcs {
                            if analysis.sink_move.contains(&(bi, si, src)) && src != dest.0 {
                                out.push(Statement::Assign(
                                    Place::Local(Local(src)),
                                    Rvalue::Use(Operand::Const(Const::Null)),
                                ));
                            }
                        }
                        changed = true;
                    }
                    Some((dest, retain, false, move_from)) => {
                        if had_dest {
                            out.push(release_one(
                                dest.0,
                                unique_destroy(interner, &local_types, &take_flags, dest.0, had_unique),
                            ));
                        }
                        for r in sink_retains {
                            out.push(r);
                        }
                        out.push(stmt);
                        if retain && analysis.assign_move.contains(&(bi, si)) {
                            let src = move_from.expect("move site implies owned local source");
                            out.push(Statement::Assign(
                                Place::Local(src),
                                Rvalue::Use(Operand::Const(Const::Null)),
                            ));
                        } else if retain {
                            out.push(Statement::Retain(Operand::Copy(Place::Local(dest))));
                        }
                        for n in sink_nulls {
                            out.push(n);
                        }
                        for src in container_srcs {
                            if analysis.sink_move.contains(&(bi, si, src)) && src != dest.0 {
                                out.push(Statement::Assign(
                                    Place::Local(Local(src)),
                                    Rvalue::Use(Operand::Const(Const::Null)),
                                ));
                            }
                        }
                        changed = true;
                    }
                    None => {
                        let mut had_sink = !sink_retains.is_empty() || !sink_nulls.is_empty();
                        for r in sink_retains {
                            out.push(r);
                        }
                        out.push(stmt);
                        for n in sink_nulls {
                            out.push(n);
                        }
                        for src in container_srcs {
                            if analysis.sink_move.contains(&(bi, si, src)) {
                                out.push(Statement::Assign(
                                    Place::Local(Local(src)),
                                    Rvalue::Use(Operand::Const(Const::Null)),
                                ));
                                had_sink = true;
                            }
                        }
                        if had_sink {
                            changed = true;
                        }
                    }
                }

                for local in 0..tokens.len() as u32 {
                    if analysis.die_after.contains(&(bi, si, local)) {
                        let u = unique_destroy(
                            interner,
                            &local_types,
                            &take_flags,
                            local,
                            unique.get(local as usize).copied().unwrap_or(false),
                        );
                        out.extend(release_and_null(local, u));
                        tokens[local as usize] = false;
                        unique[local as usize] = false;
                        changed = true;
                    }
                }
            }
            for &local in &analysis.share_at_end[bi] {
                if dest_holds_token(&tokens, local) {
                    out.push(Statement::Retain(Operand::Copy(Place::Local(Local(local)))));
                    unique[local as usize] = false;
                    changed = true;
                }
            }
            for &local in &analysis.end_release[bi] {
                if dest_holds_token(&tokens, local) {
                    let u = unique_destroy(
                        interner,
                        &local_types,
                        &take_flags,
                        local,
                        unique.get(local as usize).copied().unwrap_or(false),
                    );
                    out.extend(release_and_null(local, u));
                    tokens[local as usize] = false;
                    unique[local as usize] = false;
                    changed = true;
                }
            }
            block.stmts = out;
        }

        func.locals.extend(extra_locals);

        insert_value_struct_moves(func, interner, &mut changed);

        if !analysis.has_await {
            insert_early_value_drops(func, interner, &mut changed);
        }
        // Last-use of an awaited handle: resume copies the result, then this drop frees the
        // future. Token flow also drops the handle at Await so AsyncComplete does not double-free.
        insert_await_resume_releases(func, interner, &mut changed);
        mark_returned_value_locals_moved(func, interner, &mut changed);

        let ret_is_ref = interner.is_rc_tracked(func.ret);
        let mut spills: Vec<LocalDecl> = Vec::new();
        let next_local = func.locals.len() as u32;
        for (bi, block) in func.blocks.iter_mut().enumerate() {
            let ret = match &block.terminator {
                Terminator::Return(v) | Terminator::AsyncComplete(v) => v.clone(),
                _ => continue,
            };
            let is_async_complete = matches!(block.terminator, Terminator::AsyncComplete(_));
            let (skip, spill_from): (Option<u32>, Option<Operand>) = match &ret {
                Some(Operand::Copy(Place::Local(l))) if is_owned(l.0) => (Some(l.0), None),
                Some(op) if ret_is_ref => (None, Some(op.clone())),
                _ => (None, None),
            };
            let skip = if let Some(op) = spill_from {
                let temp = Local(next_local + spills.len() as u32);
                spills.push(LocalDecl {
                    ty: func.ret,
                    name: None,
                    is_ref: false,
                    is_take: false,
                    is_cursor: false,
                    manual_drop: false,
                });
                block
                    .stmts
                    .push(Statement::Assign(Place::Local(temp), Rvalue::Use(op)));
                block
                    .stmts
                    .push(Statement::Retain(Operand::Copy(Place::Local(temp))));
                let spilled = Some(Operand::Copy(Place::Local(temp)));
                block.terminator = if is_async_complete {
                    Terminator::AsyncComplete(spilled)
                } else {
                    Terminator::Return(spilled)
                };
                changed = true;
                Some(temp.0)
            } else {
                skip
            };
            if let Some(tok) = analysis.token_out.get(bi) {
                let uniq = analysis.unique_out.get(bi);
                for (i, owned) in tok.iter().enumerate() {
                    if !*owned || Some(i as u32) == skip {
                        continue;
                    }
                    let u = uniq.and_then(|row| row.get(i).copied()).unwrap_or(false)
                        && take_flags.get(i).copied() != Some(true)
                        && can_unique_destroy(interner, func.locals[i].ty);
                    block.stmts.push(release_one(i as u32, u));
                    changed = true;
                }
            }
        }
        func.locals.extend(spills);
        changed
    }
}

impl MirPass for RcInsertion {
    fn name(&self) -> &'static str {
        "rc-insertion"
    }

    fn run(&self, func: &mut MirFunction, interner: &TypeInterner) -> bool {
        self.run_inner(func, interner, &dream_hir::LayoutTable::default())
    }
}

/// Intra-procedural Unique is not object uniqueness: a take param may be a copy the caller
/// still holds (field extract, still-live local). Unique-destroy would `free` under them.
fn unique_destroy(
    interner: &TypeInterner,
    local_types: &[dream_types::TypeId],
    take_flags: &[bool],
    local: u32,
    unique: bool,
) -> bool {
    unique
        && take_flags.get(local as usize) != Some(&true)
        && local_types
            .get(local as usize)
            .is_some_and(|ty| can_unique_destroy(interner, *ty))
}

fn release_one(local: u32, unique: bool) -> Statement {
    let op = Operand::Copy(Place::Local(Local(local)));
    if unique {
        Statement::ReleaseUnique(op)
    } else {
        Statement::Release(op)
    }
}

fn insert_await_resume_releases(
    func: &mut MirFunction,
    interner: &TypeInterner,
    changed: &mut bool,
) {
    let is_owned = |l: u32| is_owned_local(func, interner, l);
    let live_out = liveness::live_out(func);
    let mut resume_releases: Vec<(usize, u32)> = Vec::new();
    for block in &func.blocks {
        if let Terminator::Await {
            future,
            dest,
            resume,
        } = &block.terminator
        {
            let Operand::Copy(Place::Local(l)) = future else {
                continue;
            };
            if dest == &Some(*l) {
                continue;
            }
            if !is_owned(l.0) {
                continue;
            }
            if live_in_of(func, &live_out, resume.0 as usize).contains(&l.0) {
                continue;
            }
            resume_releases.push((resume.0 as usize, l.0));
        }
    }
    for (ri, local) in resume_releases {
        let already = func.blocks[ri]
            .stmts
            .iter()
            .any(|s| rc_op_on_local(s, local));
        if already {
            continue;
        }
        let mut stmts = Vec::with_capacity(func.blocks[ri].stmts.len() + 2);
        stmts.extend(release_and_null(local, false));
        stmts.append(&mut func.blocks[ri].stmts);
        func.blocks[ri].stmts = stmts;
        *changed = true;
    }
}

/// Last-use move for glue value structs: still-live copies/args get [`Statement::ValueRetain`];
/// last-use transfers get [`Statement::ValueKill`] (callee / dest inherits nested refs).
fn insert_value_struct_moves(func: &mut MirFunction, interner: &TypeInterner, changed: &mut bool) {
    let is_value_src = |idx: usize| {
        let d = &func.locals[idx];
        interner.is_value_type(d.ty)
            && !d.is_ref
            && d.name.is_some()
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
    let mut before_by: HashMap<usize, Vec<(usize, u32, u32)>> = HashMap::new();
    let mut after_by: HashMap<usize, Vec<(usize, u32)>> = HashMap::new();
    let mut kill_by: HashMap<usize, Vec<(usize, u32)>> = HashMap::new();
    for (bi, si, local, n) in retain_before {
        before_by.entry(bi).or_default().push((si, local, n));
    }
    for (bi, si, local) in retain_after {
        after_by.entry(bi).or_default().push((si, local));
    }
    for (bi, si, local) in kill_after {
        func.locals[local as usize].manual_drop = true;
        kill_by.entry(bi).or_default().push((si, local));
    }
    let mut blocks: Vec<usize> = before_by
        .keys()
        .chain(after_by.keys())
        .chain(kill_by.keys())
        .copied()
        .collect();
    blocks.sort_unstable();
    blocks.dedup();
    for bi in blocks {
        let before = before_by.remove(&bi).unwrap_or_default();
        let after = after_by.remove(&bi).unwrap_or_default();
        let kills = kill_by.remove(&bi).unwrap_or_default();
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

fn value_arg_locals(
    func: &MirFunction,
    stmt: &Statement,
    interner: &TypeInterner,
    is_value_src: &dyn Fn(usize) -> bool,
) -> Vec<u32> {
    let Some((take_params, args)) = sink_call_args(stmt) else {
        return Vec::new();
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
fn mark_returned_value_locals_moved(
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
    if decl.name.is_some() {
        return true;
    }
    false
}

/// Early `ValueDrop` after the last use of an owning value local. Whole-value copy-out (`dest = src`)
/// keeps copy semantics (frame teardown still drops `src`); last *read* drops immediately.
fn insert_early_value_drops(func: &mut MirFunction, interner: &TypeInterner, changed: &mut bool) {
    let live_out = liveness::live_out(func);
    let owning: Vec<u32> = (0..func.locals.len())
        .filter(|&i| is_owning_value_local(func, interner, i))
        .map(|i| i as u32)
        .collect();
    if owning.is_empty() {
        return;
    }
    let mut drop_at: Vec<(usize, usize, u32)> = Vec::new();
    for (bi, block) in func.blocks.iter().enumerate() {
        for (si, stmt) in block.stmts.iter().enumerate() {
            for &local in &owning {
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
                let used = stmt_reads_local(stmt, local) || assigns_local(stmt, local);
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
    let mut by_block: HashMap<usize, Vec<(usize, u32)>> = HashMap::new();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use crate::Callee;
    use dream_types::{DefKind, TypeCtx};

    fn point_ty(ctx: &mut TypeCtx) -> dream_types::TypeId {
        let vs_def = ctx.register(DefKind::Struct, "Point", vec![]);
        ctx.defs.mark_value(vs_def);
        ctx.interner.mark_value_def(vs_def);
        let point = ctx.interner.struct_ty(vs_def, vec![]);
        ctx.interner.set_value_layout(point, 8, 4);
        point
    }

    #[test]
    fn last_use_value_assign_kills_source() {
        let mut ctx = TypeCtx::new();
        let point = point_ty(&mut ctx);
        let mut b = FunctionBuilder::new("f", ctx.interner.void());
        let s = b.new_local(point, Some("s".into()));
        let t = b.new_local(point, Some("t".into()));
        b.assign(Place::Local(t), Rvalue::Use(Operand::Copy(Place::Local(s))));
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &ctx.interner));
        let kills: Vec<u32> = func.blocks[0]
            .stmts
            .iter()
            .filter_map(|st| match st {
                Statement::ValueKill(l) => Some(l.0),
                _ => None,
            })
            .collect();
        assert_eq!(kills, vec![s.0], "last-use dest=src should ValueKill src");
        assert!(func.locals[s.0 as usize].manual_drop);
        let retains = func.blocks[0]
            .stmts
            .iter()
            .filter(|st| matches!(st, Statement::ValueRetain(_)))
            .count();
        assert_eq!(retains, 0);
    }

    #[test]
    fn still_live_value_assign_retains_dest() {
        let mut ctx = TypeCtx::new();
        let point = point_ty(&mut ctx);
        let mut b = FunctionBuilder::new("f", ctx.interner.void());
        let s = b.new_local(point, Some("s".into()));
        let t = b.new_local(point, Some("t".into()));
        b.assign(Place::Local(t), Rvalue::Use(Operand::Copy(Place::Local(s))));
        b.assign(Place::Local(s), Rvalue::Use(Operand::Copy(Place::Local(s))));
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &ctx.interner));
        let has_retain_t = func.blocks[0]
            .stmts
            .iter()
            .any(|st| matches!(st, Statement::ValueRetain(l) if l.0 == t.0));
        assert!(has_retain_t, "still-live dest=src should ValueRetain dest");
    }

    #[test]
    fn last_use_value_call_kills_arg() {
        let mut ctx = TypeCtx::new();
        let point = point_ty(&mut ctx);
        let take_def = ctx.register(DefKind::Function, "take", vec![]);
        let mut b = FunctionBuilder::new("f", ctx.interner.void());
        let s = b.new_local(point, Some("s".into()));
        b.push(Statement::Call {
            callee: Callee {
                def: take_def,
                args: vec![],
                ret: ctx.interner.void(),
                take_params: vec![true],
            },
            args: vec![Operand::Copy(Place::Local(s))],
        });
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &ctx.interner));
        let has_kill = func.blocks[0]
            .stmts
            .iter()
            .any(|st| matches!(st, Statement::ValueKill(l) if l.0 == s.0));
        let has_retain = func.blocks[0]
            .stmts
            .iter()
            .any(|st| matches!(st, Statement::ValueRetain(l) if l.0 == s.0));
        assert!(has_kill, "last-use call arg should ValueKill");
        assert!(!has_retain, "last-use call arg should not ValueRetain");
    }

    #[test]
    fn still_live_value_call_retains_arg() {
        let mut ctx = TypeCtx::new();
        let point = point_ty(&mut ctx);
        let take_def = ctx.register(DefKind::Function, "take", vec![]);
        let mut b = FunctionBuilder::new("f", ctx.interner.void());
        let s = b.new_local(point, Some("s".into()));
        b.push(Statement::Call {
            callee: Callee {
                def: take_def,
                args: vec![],
                ret: ctx.interner.void(),
                take_params: vec![true],
            },
            args: vec![Operand::Copy(Place::Local(s))],
        });
        b.assign(Place::Local(s), Rvalue::Use(Operand::Copy(Place::Local(s))));
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &ctx.interner));
        let has_retain = func.blocks[0]
            .stmts
            .iter()
            .any(|st| matches!(st, Statement::ValueRetain(l) if l.0 == s.0));
        assert!(has_retain, "still-live call arg should ValueRetain");
    }

    #[test]
    fn still_live_iface_arg_retains() {
        let mut ctx = TypeCtx::new();
        let str_ty = ctx.interner.string();
        let sig = ctx.interner.func(vec![str_ty, str_ty], ctx.interner.void());
        let mut b = FunctionBuilder::new("f", ctx.interner.void());
        let recv = b.new_local(str_ty, Some("h".into()));
        let rec = b.new_local(str_ty, Some("r".into()));
        b.push(Statement::InterfaceCall {
            receiver: Operand::Copy(Place::Local(recv)),
            iface_id: 0,
            method_slot: 0,
            sig,
            args: vec![Operand::Copy(Place::Local(rec))],
        });
        b.assign(
            Place::Local(rec),
            Rvalue::Use(Operand::Copy(Place::Local(rec))),
        );
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &ctx.interner));
        let has_retain = func.blocks[0].stmts.iter().any(
            |st| matches!(st, Statement::Retain(Operand::Copy(Place::Local(l))) if l.0 == rec.0),
        );
        assert!(has_retain, "still-live interface arg should Retain");
    }

    fn class_ty(ctx: &mut TypeCtx) -> (dream_types::DefId, dream_types::TypeId) {
        let def = ctx.register(DefKind::Struct, "User", vec![]);
        let ty = ctx.interner.struct_ty(def, vec![]);
        (def, ty)
    }

    fn count_rc(func: &MirFunction) -> (usize, usize) {
        let mut retains = 0;
        let mut releases = 0;
        for b in &func.blocks {
            for s in &b.stmts {
                match s {
                    Statement::Retain(_) => retains += 1,
                    Statement::Release(_) | Statement::ReleaseUnique(_) => releases += 1,
                    _ => {}
                }
            }
        }
        (retains, releases)
    }

    #[test]
    fn birth_borrow_falls_off_block_one_release_zero_retain() {
        let mut ctx = TypeCtx::new();
        let (def, ty) = class_ty(&mut ctx);
        let peek = ctx.register(DefKind::Function, "peek", vec![]);
        let mut b = FunctionBuilder::new("f", ctx.interner.void());
        let x = b.new_local(ty, Some("x".into()));
        b.assign(
            Place::Local(x),
            Rvalue::New {
                def,
                ty,
                ctor: None,
                args: vec![],
            },
        );
        b.push(Statement::Call {
            callee: Callee {
                def: peek,
                args: vec![],
                ret: ctx.interner.void(),
                take_params: vec![false],
            },
            args: vec![Operand::Copy(Place::Local(x))],
        });
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &ctx.interner));
        let (retains, releases) = count_rc(&func);
        assert_eq!(retains, 0, "borrow should not retain");
        assert_eq!(releases, 1, "token dies once: {:?}", func.blocks[0].stmts);
    }

    #[test]
    fn last_use_assign_forwards_token_without_retain() {
        let mut ctx = TypeCtx::new();
        let (def, ty) = class_ty(&mut ctx);
        let mut b = FunctionBuilder::new("f", ctx.interner.void());
        let x = b.new_local(ty, Some("x".into()));
        let y = b.new_local(ty, Some("y".into()));
        b.assign(
            Place::Local(x),
            Rvalue::New {
                def,
                ty,
                ctor: None,
                args: vec![],
            },
        );
        b.assign(Place::Local(y), Rvalue::Use(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &ctx.interner));
        let (retains, releases) = count_rc(&func);
        assert_eq!(retains, 0);
        assert_eq!(releases, 1, "only y dies: {:?}", func.blocks[0].stmts);
        let nulls = func.blocks[0]
            .stmts
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    Statement::Assign(_, Rvalue::Use(Operand::Const(Const::Null)))
                )
            })
            .count();
        assert_eq!(nulls, 1, "x is consumed");
    }

    #[test]
    fn still_live_alias_retains() {
        let mut ctx = TypeCtx::new();
        let (def, ty) = class_ty(&mut ctx);
        let mut b = FunctionBuilder::new("f", ctx.interner.void());
        let x = b.new_local(ty, Some("x".into()));
        let y = b.new_local(ty, Some("y".into()));
        b.assign(
            Place::Local(x),
            Rvalue::New {
                def,
                ty,
                ctor: None,
                args: vec![],
            },
        );
        b.assign(Place::Local(y), Rvalue::Use(Operand::Copy(Place::Local(x))));
        let take = ctx.register(DefKind::Function, "take", vec![]);
        b.push(Statement::Call {
            callee: Callee {
                def: take,
                args: vec![],
                ret: ctx.interner.void(),
                take_params: vec![true],
            },
            args: vec![Operand::Copy(Place::Local(y))],
        });
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(x)))));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &ctx.interner));
        let (retains, _) = count_rc(&func);
        assert_eq!(retains, 1, "copy while x lives must retain");
    }

    #[test]
    fn unbalanced_if_releases_on_kept_arm() {
        let mut ctx = TypeCtx::new();
        let (def, ty) = class_ty(&mut ctx);
        let take = ctx.register(DefKind::Function, "take", vec![]);
        let mut b = FunctionBuilder::new("f", ctx.interner.void());
        let x = b.new_local(ty, Some("x".into()));
        let c = b.new_local(ctx.interner.bool(), Some("c".into()));
        let then_blk = b.new_block();
        let else_blk = b.new_block();
        let join = b.new_block();
        b.assign(
            Place::Local(x),
            Rvalue::New {
                def,
                ty,
                ctor: None,
                args: vec![],
            },
        );
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(c)),
            then_blk,
            else_blk,
        });
        b.switch_to(then_blk);
        b.push(Statement::Call {
            callee: Callee {
                def: take,
                args: vec![],
                ret: ctx.interner.void(),
                take_params: vec![true],
            },
            args: vec![Operand::Copy(Place::Local(x))],
        });
        b.terminate(Terminator::Goto(join));
        b.switch_to(else_blk);
        b.terminate(Terminator::Goto(join));
        b.switch_to(join);
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &ctx.interner));
        let (retains, releases) = count_rc(&func);
        assert_eq!(retains, 0, "linear take vs unused arm");
        assert_eq!(releases, 1, "else arm consumes leftover token");
        let else_rel = func.blocks[else_blk.0 as usize]
            .stmts
            .iter()
            .any(|s| matches!(s, Statement::Release(_) | Statement::ReleaseUnique(_)));
        assert!(else_rel, "release is on the arm that still held the token");
    }

    #[test]
    fn loop_live_local_does_not_move_into_sink() {
        let mut ctx = TypeCtx::new();
        let (def, ty) = class_ty(&mut ctx);
        let take = ctx.register(DefKind::Function, "take", vec![]);
        let mut b = FunctionBuilder::new("f", ctx.interner.void());
        let x = b.new_local(ty, Some("x".into()));
        let c = b.new_local(ctx.interner.bool(), Some("c".into()));
        let header = b.new_block();
        let body = b.new_block();
        let exit = b.new_block();
        b.assign(
            Place::Local(x),
            Rvalue::New {
                def,
                ty,
                ctor: None,
                args: vec![],
            },
        );
        b.terminate(Terminator::Goto(header));
        b.switch_to(header);
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(c)),
            then_blk: body,
            else_blk: exit,
        });
        b.switch_to(body);
        b.push(Statement::Call {
            callee: Callee {
                def: take,
                args: vec![],
                ret: ctx.interner.void(),
                take_params: vec![true],
            },
            args: vec![Operand::Copy(Place::Local(x))],
        });
        b.terminate(Terminator::Goto(header));
        b.switch_to(exit);
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &ctx.interner));
        let retains = func
            .blocks
            .iter()
            .flat_map(|bb| &bb.stmts)
            .filter(|s| matches!(s, Statement::Retain(_)))
            .count();
        assert!(
            retains >= 1,
            "back-edge keeps x live so take copies: {:?}",
            func.blocks
        );
        let body_nulls = func.blocks[body.0 as usize]
            .stmts
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    Statement::Assign(_, Rvalue::Use(Operand::Const(Const::Null)))
                )
            })
            .count();
        assert_eq!(body_nulls, 0, "must not move x inside the loop");
    }

    #[test]
    fn rebind_evaluates_rhs_before_release() {
        let mut ctx = TypeCtx::new();
        let (def, ty) = class_ty(&mut ctx);
        let mutate = ctx.register(DefKind::Function, "mutate_and_return", vec![]);
        let mut b = FunctionBuilder::new("f", ctx.interner.void());
        let x = b.new_local(ty, Some("x".into()));
        b.assign(
            Place::Local(x),
            Rvalue::New {
                def,
                ty,
                ctor: None,
                args: vec![],
            },
        );
        b.assign(
            Place::Local(x),
            Rvalue::Call {
                callee: Callee {
                    def: mutate,
                    args: vec![],
                    ret: ty,
                    take_params: vec![false],
                },
                args: vec![Operand::Copy(Place::Local(x))],
            },
        );
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &ctx.interner));
        let stmts = &func.blocks[0].stmts;
        let call_at = stmts
            .iter()
            .position(|s| matches!(s, Statement::Assign(_, Rvalue::Call { .. })));
        let rel_at = stmts.iter().position(
            |s| matches!(s, Statement::Release(Operand::Copy(Place::Local(l))) | Statement::ReleaseUnique(Operand::Copy(Place::Local(l))) if *l == x),
        );
        assert!(call_at.is_some() && rel_at.is_some(), "{:?}", stmts);
        assert!(
            call_at.unwrap() < rel_at.unwrap(),
            "Release(x) must follow tmp = mutate_and_return(x): {:?}",
            stmts
        );
    }

    #[test]
    fn hidden_borrow_string_not_released_mid_block_after_call() {
        let i = dream_types::TypeInterner::new();
        let peek = dream_types::DefId(0);
        let mut b = FunctionBuilder::new("f", i.void());
        let s = b.new_local(i.string(), Some("s".into()));
        let tmp = b.new_local(i.int(), Some("tmp".into()));
        b.assign(
            Place::Local(s),
            Rvalue::Use(Operand::Const(Const::Str("x".into()))),
        );
        b.push(Statement::Call {
            callee: Callee {
                def: peek,
                args: vec![],
                ret: i.void(),
                take_params: vec![false],
            },
            args: vec![Operand::Copy(Place::Local(s))],
        });
        b.assign(
            Place::Local(tmp),
            Rvalue::Binary(
                crate::BinOp::Add,
                Operand::Const(Const::Int(1)),
                Operand::Const(Const::Int(2)),
            ),
        );
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        RcInsertion.run(&mut func, &i);
        let stmts = &func.blocks[0].stmts;
        let call_at = stmts
            .iter()
            .position(|s| matches!(s, Statement::Call { .. }))
            .unwrap();
        let add_at = stmts
            .iter()
            .position(|s| matches!(s, Statement::Assign(_, Rvalue::Binary(..))))
            .unwrap();
        let mid = stmts.iter().enumerate().any(|(idx, st)| {
            idx > call_at
                && idx < add_at
                && matches!(st, Statement::Release(Operand::Copy(Place::Local(l))) if *l == s)
        });
        assert!(
            !mid,
            "string must not be released between hidden-borrow call and later work: {:?}",
            stmts
        );
    }

    #[test]
    fn unique_new_uses_release_unique() {
        let mut ctx = TypeCtx::new();
        let (def, ty) = class_ty(&mut ctx);
        let mut b = FunctionBuilder::new("f", ctx.interner.void());
        let x = b.new_local(ty, Some("x".into()));
        b.assign(
            Place::Local(x),
            Rvalue::New {
                def,
                ty,
                ctor: None,
                args: vec![],
            },
        );
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &ctx.interner));
        let has_unique = func.blocks[0]
            .stmts
            .iter()
            .any(|s| matches!(s, Statement::ReleaseUnique(_)));
        let has_retain = func.blocks[0]
            .stmts
            .iter()
            .any(|s| matches!(s, Statement::Retain(_)));
        assert!(
            has_unique,
            "unique birth should ReleaseUnique: {:?}",
            func.blocks[0].stmts
        );
        assert!(!has_retain);
    }

    #[test]
    fn last_use_field_store_nulls_without_retain() {
        let mut ctx = TypeCtx::new();
        let (def, ty) = class_ty(&mut ctx);
        let mut b = FunctionBuilder::new("f", ctx.interner.void());
        let obj = b.new_local(ty, Some("obj".into()));
        let x = b.new_local(ty, Some("x".into()));
        b.assign(
            Place::Local(obj),
            Rvalue::New {
                def,
                ty,
                ctor: None,
                args: vec![],
            },
        );
        b.assign(
            Place::Local(x),
            Rvalue::New {
                def,
                ty,
                ctor: None,
                args: vec![],
            },
        );
        b.assign(
            Place::Field {
                base: obj,
                field: 0,
            },
            Rvalue::Use(Operand::Copy(Place::Local(x))),
        );
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &ctx.interner));
        let (retains, _) = count_rc(&func);
        assert_eq!(
            retains, 0,
            "last-use field store is a move: {:?}",
            func.blocks[0].stmts
        );
        let null_x = func.blocks[0].stmts.iter().any(|s| {
            matches!(
                s,
                Statement::Assign(Place::Local(l), Rvalue::Use(Operand::Const(Const::Null)))
                    if *l == x
            )
        });
        assert!(
            null_x,
            "source nulled after container move: {:?}",
            func.blocks[0].stmts
        );
    }

    #[test]
    fn still_live_copy_is_shared_not_unique_destroy() {
        let mut ctx = TypeCtx::new();
        let (def, ty) = class_ty(&mut ctx);
        let mut b = FunctionBuilder::new("f", ctx.interner.void());
        let x = b.new_local(ty, Some("x".into()));
        let y = b.new_local(ty, Some("y".into()));
        b.assign(
            Place::Local(x),
            Rvalue::New {
                def,
                ty,
                ctor: None,
                args: vec![],
            },
        );
        b.assign(Place::Local(y), Rvalue::Use(Operand::Copy(Place::Local(x))));
        b.assign(Place::Local(x), Rvalue::Use(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &ctx.interner));
        let (retains, _) = count_rc(&func);
        assert!(
            retains >= 1,
            "still-live alias retains: {:?}",
            func.blocks[0].stmts
        );
    }

    #[test]
    fn string_never_release_unique() {
        let i = dream_types::TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.void());
        let s = b.new_local(i.string(), Some("s".into()));
        b.assign(
            Place::Local(s),
            Rvalue::Use(Operand::Const(Const::Str("x".into()))),
        );
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &i));
        let uniq = func.blocks[0]
            .stmts
            .iter()
            .any(|st| matches!(st, Statement::ReleaseUnique(_)));
        assert!(
            !uniq,
            "strings stay on ordinary release: {:?}",
            func.blocks[0].stmts
        );
    }
}
