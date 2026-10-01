//! [`RcInsertion`]: make reference ownership explicit in MIR via compile-time tokens.

use super::is_borrowed_copy;
use super::value::{
    insert_complete_value_drops, insert_early_value_drops, insert_value_struct_moves,
    mark_returned_value_locals_moved,
};
use super::modref::ModRefTable;
use super::liveness::{self, live_after_stmt, stmt_reads_local};
use super::tokens::{
    apply_stmt_tokens, dest_holds_token, funcbox_env_rc_roots, is_owned_local,
    leftover_alias_parent, leftover_keep, leftover_order, move_source, needs_rebind_temp,
    null_local, rc_op_on_local, release_and_null,
    take_arg_effects, terminator_reads_local, TokenAnalysis,
};
use super::uniqueness::{
    apply_stmt_unique, can_unique_destroy, constructed_payload_locals, container_move_locals,
    mark_container_move,
};
use crate::passes::cfg;
use crate::passes::MirPass;
use crate::{
    Const, Global, Local, LocalDecl, MirFunction, Operand, Place, Rvalue, Statement, Terminator,
};
use dream_types::{DefId, TypeInterner};
use indexmap::{IndexMap, IndexSet};

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
        holds: &IndexSet<DefId>,
        modref: &ModRefTable,
    ) -> bool {
        RcInsertion.run_inner(func, interner, layouts, holds, modref)
    }

    fn run_inner(
        &self,
        func: &mut MirFunction,
        interner: &TypeInterner,
        layouts: &dream_hir::LayoutTable,
        holds: &IndexSet<DefId>,
        modref: &ModRefTable,
    ) -> bool {
        super::cursor::infer_cursors(func, interner, layouts, modref);

        let local_is_ref: Vec<bool> = func
            .locals
            .iter()
            .map(|d| interner.is_rc_tracked(d.ty))
            .collect();
        let analysis = TokenAnalysis::analyze(func, interner, layouts, holds, modref);
        let leftover_parent = leftover_alias_parent(func, interner, true);
        let env_defer = funcbox_env_rc_roots(func, interner);
        let start_keep: Vec<IndexSet<u32>> = analysis
            .start_release
            .iter()
            .map(|s| leftover_keep(func, s.iter().copied()))
            .collect();
        let end_keep: Vec<IndexSet<u32>> = analysis
            .end_release
            .iter()
            .map(|s| leftover_keep(func, s.iter().copied()))
            .collect();
        let mut die_keep: IndexMap<(usize, usize), IndexSet<u32>> = IndexMap::new();
        {
            let mut groups: IndexMap<(usize, usize), Vec<u32>> = IndexMap::new();
            for &(bi, si, local) in &analysis.die_after {
                groups.entry((bi, si)).or_default().push(local);
            }
            for (k, ids) in groups {
                die_keep.insert(k, leftover_keep(func, ids));
            }
        }
        let n_orig = func.locals.len() as u32;
        let owned_flags: Vec<bool> = (0..func.locals.len() as u32)
            .map(|l| is_owned_local(func, interner, l))
            .collect();
        let is_owned = |l: u32| owned_flags.get(l as usize).copied().unwrap_or(false);
        let mut changed = false;

        let mut realloc_readers: IndexMap<(usize, usize), Vec<u32>> = IndexMap::new();
        let mut slot_readers: IndexMap<SlotId, Vec<u32>> = IndexMap::new();
        let live_out_rc = liveness::live_out(func);
        let is_async = func.is_async;
        // Resume blocks get their awaited future's Release from `insert_await_resume_releases`,
        // which runs after this loop and treats an existing `x = null` as "already handled".
        let mut resume_futures: Vec<IndexSet<u32>> = vec![IndexSet::new(); func.blocks.len()];
        for block in &func.blocks {
            if let Terminator::Await {
                future: Operand::Copy(Place::Local(f)),
                resume,
                ..
            } = &block.terminator
            {
                resume_futures[resume.0 as usize].insert(f.0);
            }
        }
        let in_loop: IndexSet<usize> = cfg::natural_loops(func)
            .iter()
            .flat_map(|lp| lp.body.iter().map(|b| b.0 as usize))
            .collect();
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
                let Statement::Assign(dest_place, Rvalue::ArrayRealloc { array, .. }) = stmt else {
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
            for local in leftover_order(
                &leftover_parent,
                analysis.start_release[bi].iter().copied(),
                &env_defer,
            ) {
                // Join/loop-header leftover: the other pred may have copied this pointer into a
                // still-live container. Unique destroy ignores RC and would free that copy.
                if should_release_leftover(&start_keep[bi], n_orig, local)
                    && leftover_env_ok(local, &tokens, &env_defer)
                {
                    out.extend(release_and_null(local, false));
                } else {
                    out.push(null_local(local));
                }
                if (local as usize) < tokens.len() {
                    tokens[local as usize] = false;
                    unique[local as usize] = false;
                }
                changed = true;
            }
            if let Some(d) = analysis.await_resume_dest.get(bi).copied().flatten() {
                if (d as usize) < tokens.len() && is_owned(d) {
                    tokens[d as usize] = true;
                    unique[d as usize] = true;
                }
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
                let dest_had_token = ref_dest
                    .as_ref()
                    .map(|(d, _, _, _)| dest_holds_token(&tokens, d.0))
                    .unwrap_or(false);
                // Loop-header token join treats the entry pred as empty, so a loop-carried
                // owned local can overwrite a still-resident pointer with `had_dest` false.
                let drop_previous = dest_had_token
                    || ref_dest
                        .as_ref()
                        .is_some_and(|(d, _, _, _)| is_owned(d.0) && in_loop.contains(&bi));
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
                // Whether each container-store source actually holds a `+1` of its own here.
                // `sink_move` says only that the token flow is done with the local; this says there
                // was a token to hand over, and a copy of a string literal is the first without
                // being the second. A store that adopts from one leaves the object with no
                // owner to release it.
                let src_owns: Vec<u32> = container_srcs
                    .iter()
                    .copied()
                    .filter(|&src| dest_holds_token(&tokens, src))
                    .collect();
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
                    Some((dest, retain, true, _)) if drop_previous => {
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
                            unique_destroy(
                                interner,
                                &local_types,
                                &take_flags,
                                dest.0,
                                dest_had_token && had_unique,
                            ),
                        ));
                        out.push(Statement::Assign(
                            Place::Local(dest),
                            Rvalue::Use(Operand::Copy(Place::Local(tmp))),
                        ));
                        if retain {
                            out.push(Statement::Retain(Operand::Copy(Place::Local(dest))));
                        }
                        // The temp handed its reference to `dest`. Leaving it set lets copy
                        // propagation rewrite later uses of `dest` back onto it, and an async
                        // frame spills it, so `drop_*` would release the pointer a second time.
                        out.push(null_local(tmp.0));
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
                        if drop_previous {
                            out.push(release_one(
                                dest.0,
                                unique_destroy(
                                    interner,
                                    &local_types,
                                    &take_flags,
                                    dest.0,
                                    dest_had_token && had_unique,
                                ),
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
                        let mut stmt = stmt;
                        let moved: Vec<u32> = container_srcs
                            .into_iter()
                            .filter(|src| analysis.sink_move.contains(&(bi, si, *src)))
                            .collect();
                        for &src in &moved {
                            if src_owns.contains(&src) {
                                mark_container_move(&mut stmt, src);
                            }
                        }
                        out.push(stmt);
                        for n in sink_nulls {
                            out.push(n);
                        }
                        for src in moved {
                            out.push(null_local(src));
                            had_sink = true;
                        }
                        if had_sink {
                            changed = true;
                        }
                    }
                }

                let dying: Vec<u32> = (0..tokens.len() as u32)
                    .filter(|&local| analysis.die_after.contains(&(bi, si, local)))
                    .collect();
                for local in leftover_order(&leftover_parent, dying, &env_defer) {
                    let u = unique_destroy(
                        interner,
                        &local_types,
                        &take_flags,
                        local,
                        unique.get(local as usize).copied().unwrap_or(false),
                    );
                    let one = IndexSet::from([local]);
                    let keep = die_keep.get(&(bi, si)).unwrap_or(&one);
                    if should_release_leftover(keep, n_orig, local)
                        && leftover_env_ok(local, &tokens, &env_defer)
                    {
                        out.extend(release_and_null(local, u));
                    } else {
                        out.push(null_local(local));
                    }
                    tokens[local as usize] = false;
                    unique[local as usize] = false;
                    changed = true;
                }
            }
            for &local in &analysis.share_at_end[bi] {
                unique[local as usize] = false;
            }
            for local in leftover_order(
                &leftover_parent,
                analysis.end_release[bi].iter().copied(),
                &env_defer,
            ) {
                if dest_holds_token(&tokens, local) {
                    // Leftover may run after a field/Result store that retained an alias.
                    // ReleaseUnique ignores RC and would free that copy (`JsonValue.get`,
                    // `Result.Ok(from_json)`).
                    // Do not null an Await dest: resume is a C-only store, so `x = null`
                    // here lets SCCP prove `x` is null in the resume block.
                    let clobber_await_dest = matches!(
                        &block.terminator,
                        Terminator::Await {
                            future: Operand::Copy(Place::Local(f)),
                            dest: Some(d),
                            ..
                        } if *d != *f && d.0 == local
                    );
                    let rel = should_release_leftover(&end_keep[bi], n_orig, local)
                        && leftover_env_ok(local, &tokens, &env_defer);
                    if clobber_await_dest {
                        if rel {
                            out.push(release_one(local, false));
                        }
                    } else if rel {
                        out.extend(release_and_null(local, false));
                    } else {
                        out.push(null_local(local));
                    }
                    tokens[local as usize] = false;
                    unique[local as usize] = false;
                    changed = true;
                }
            }
            // A poll frame spills every local and `drop_*` releases every non-null ref slot, so a
            // dead local that no longer owns its pointer (its token moved into a sink or to a
            // rebound alias) would be released a second time when the future is destroyed.
            if is_async {
                let await_dest = match &block.terminator {
                    Terminator::Await { dest: Some(d), .. } => Some(d.0),
                    _ => None,
                };
                for local in 0..tokens.len() as u32 {
                    if dest_holds_token(&tokens, local)
                        || Some(local) == await_dest
                        || resume_futures[bi].contains(&local)
                        || terminator_reads_local(&block.terminator, local)
                        || !local_is_ref.get(local as usize).copied().unwrap_or(false)
                        || live_out_rc[bi].contains(&local)
                    {
                        continue;
                    }
                    let already_null = out
                        .iter()
                        .rev()
                        .find_map(|s| match s {
                            Statement::Assign(Place::Local(l), rv) if l.0 == local => {
                                Some(matches!(rv, Rvalue::Use(Operand::Const(Const::Null))))
                            }
                            _ => None,
                        })
                        .unwrap_or(false);
                    if already_null {
                        continue;
                    }
                    out.push(null_local(local));
                    changed = true;
                }
            }
            block.stmts = out;
        }

        func.locals.extend(extra_locals);

        insert_value_struct_moves(func, interner, &mut changed);

        // Skip last-use drops in Await blocks: a host may still hold nested pointers until resume.
        insert_early_value_drops(func, interner, &mut changed, analysis.has_await);
        // Last-use of an awaited handle: resume copies the result, then this drop frees the
        // future. Token flow also drops the handle at Await so AsyncComplete does not double-free.
        insert_await_resume_releases(func, interner, &mut changed);
        mark_returned_value_locals_moved(func, interner, &mut changed);

        let ret_is_ref = interner.is_rc_tracked(func.ret);
        let mut spills: Vec<LocalDecl> = Vec::new();
        let next_local = func.locals.len() as u32;
        for bi in 0..func.blocks.len() {
            let ret = match &func.blocks[bi].terminator {
                Terminator::Return(v) | Terminator::AsyncComplete(v) => v.clone(),
                _ => continue,
            };
            let is_async_complete =
                matches!(func.blocks[bi].terminator, Terminator::AsyncComplete(_));
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
                func.blocks[bi]
                    .stmts
                    .push(Statement::Assign(Place::Local(temp), Rvalue::Use(op)));
                func.blocks[bi]
                    .stmts
                    .push(Statement::Retain(Operand::Copy(Place::Local(temp))));
                let spilled = Some(Operand::Copy(Place::Local(temp)));
                func.blocks[bi].terminator = if is_async_complete {
                    Terminator::AsyncComplete(spilled)
                } else {
                    Terminator::Return(spilled)
                };
                changed = true;
                Some(temp.0)
            } else {
                skip
            };
            if is_async_complete {
                insert_complete_value_drops(func, interner, bi, skip, &mut changed);
            }
            // Tokens still owned at Return/AsyncComplete (not destroyed earlier) are released here.
            // Classes already dropped on the arms; sweeping every RC local would double-free
            // at joins (`unbalanced_if_releases_on_kept_arm`).
            let tokens = analysis.token_out.get(bi);
            let nloc = func.locals.len().min(tokens.map(|t| t.len()).unwrap_or(0));
            let mut ret_drop: Vec<u32> = Vec::new();
            for i in 0..nloc {
                let local = i as u32;
                if Some(local) == skip || !is_owned_local(func, interner, local) {
                    continue;
                }
                if !tokens.and_then(|row| row.get(i).copied()).unwrap_or(false) {
                    continue;
                }
                // A last-use field/index store in *this* block already took the token.
                // Loop-carried `token_out` can still look owned after that move. A sink in
                // another block must not suppress leftover here (last-iteration future/funcbox).
                if analysis
                    .sink_move
                    .iter()
                    .any(|&(b, _, l)| b == bi && l == local)
                {
                    continue;
                }
                ret_drop.push(local);
            }
            let ret_keep = leftover_keep(func, ret_drop.iter().copied());
            let token_row = tokens.cloned().unwrap_or_default();
            for local in leftover_order(&leftover_parent, ret_drop.iter().copied(), &env_defer) {
                if should_release_leftover(&ret_keep, n_orig, local)
                    && leftover_env_ok(local, &token_row, &env_defer)
                {
                    func.blocks[bi].stmts.extend(release_and_null(local, false));
                } else {
                    func.blocks[bi].stmts.push(null_local(local));
                }
                changed = true;
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
        self.run_inner(
            func,
            interner,
            &dream_hir::LayoutTable::default(),
            &IndexSet::new(),
            &ModRefTable::default(),
        )
    }
}

/// Intra-procedural Unique is not object uniqueness: a take param may be a copy the caller
/// still holds (field extract, still-live local). Unique-destroy would `free` under them.
fn should_release_leftover(keep: &IndexSet<u32>, n_orig: u32, local: u32) -> bool {
    keep.contains(&local) || local >= n_orig
}

/// Leftover of a funcbox env releases whenever this local still holds the env's token.
/// `funcbox_new` retains the array on the box's behalf, so that is a 2→1 step while the box
/// lives and the typed 1→0 last-drop once it is gone. Gating it on the box's own leftover
/// instead leaked the leaf middleware env, whose box is released in another function.
fn leftover_env_ok(local: u32, tokens: &[bool], env_defer: &IndexSet<u32>) -> bool {
    if !env_defer.contains(&local) {
        return true;
    }
    dest_holds_token(tokens, local)
}

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

fn resume_uses_owned(block: &crate::BasicBlock, local: u32) -> bool {
    for stmt in &block.stmts {
        if rc_op_on_local(stmt, local) {
            continue;
        }
        if stmt_reads_local(stmt, local) {
            return true;
        }
    }
    match &block.terminator {
        Terminator::Await {
            future: Operand::Copy(Place::Local(f)),
            ..
        } if f.0 == local => true,
        Terminator::Return(Some(Operand::Copy(Place::Local(f))))
        | Terminator::AsyncComplete(Some(Operand::Copy(Place::Local(f))))
            if f.0 == local =>
        {
            true
        }
        _ => false,
    }
}

fn insert_await_resume_releases(
    func: &mut MirFunction,
    interner: &TypeInterner,
    changed: &mut bool,
) {
    let is_owned = |l: u32| is_owned_local(func, interner, l);
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
            // Token analysis drops the future at Await. Post-insertion liveness still
            // treats it as live into resume when the next loop body's drop_previous
            // reads it; skipping here leaks the last iteration's future.
            if resume_uses_owned(&func.blocks[resume.0 as usize], l.0) {
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

#[cfg(test)]
#[path = "insertion_tests.rs"]
mod tests;
