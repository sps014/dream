use super::super::{
    is_borrowed_copy,
    tokens::{
        apply_stmt_tokens, dest_holds_token, leftover_order, move_source, needs_rebind_temp,
        null_local, release_and_null, take_arg_effects, terminator_reads_local,
    },
    uniqueness::{container_move_locals, mark_container_move},
};
use super::{
    helpers::{leftover_env_ok, release_one, should_release_leftover},
    prepare::State,
};
use crate::{Const, Local, LocalDecl, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;
use indexmap::IndexSet;

pub(super) fn insert(func: &mut MirFunction, interner: &TypeInterner, state: &State) -> bool {
    let local_is_ref = &state.local_is_ref;
    let analysis = &state.analysis;
    let leftover_parent = &state.leftover_parent;
    let env_defer = &state.env_defer;
    let start_keep = &state.start_keep;
    let end_keep = &state.end_keep;
    let die_keep = &state.die_keep;
    let n_orig = state.n_orig;
    let owned_flags = &state.owned_flags;
    let realloc_readers = &state.realloc_readers;
    let live_out_rc = &state.live_out_rc;
    let is_async = state.is_async;
    let resume_futures = &state.resume_futures;
    let in_loop = &state.in_loop;
    let is_owned = |l: u32| owned_flags.get(l as usize).copied().unwrap_or(false);
    let mut changed = false;
    let local_types: Vec<dream_types::TypeId> = func.locals.iter().map(|d| d.ty).collect();
    let mut extra_locals: Vec<LocalDecl> = Vec::new();

    let temp_base = func.locals.len() as u32;
    for (bi, block) in func.blocks.iter_mut().enumerate() {
        let mut tokens = analysis.token_in[bi].clone();
        let mut out: Vec<Statement> = Vec::with_capacity(block.stmts.len() + 8);
        for local in leftover_order(
            leftover_parent,
            analysis.start_release[bi].iter().copied(),
            env_defer,
        ) {
            // Join/loop-header leftover: the other pred may have copied this pointer into a
            // still-live container. Unique destroy ignores RC and would free that copy.
            if should_release_leftover(&start_keep[bi], n_orig, local)
                && leftover_env_ok(local, &tokens, env_defer)
            {
                out.extend(release_and_null(local));
            } else {
                out.push(null_local(local));
            }
            if (local as usize) < tokens.len() {
                tokens[local as usize] = false;
            }
            changed = true;
        }
        if let Some(d) = analysis.await_resume_dest.get(bi).copied().flatten() {
            if (d as usize) < tokens.len() && is_owned(d) {
                tokens[d as usize] = true;
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
                    pre_releases.extend(release_and_null(x));
                }
            }
            for r in &pre_releases {
                if let Statement::Release(Operand::Copy(Place::Local(l))) = r {
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

            let (sink_retains, sink_nulls) =
                take_arg_effects(&stmt, &is_owned, local_is_ref, |local| {
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
                    out.push(release_one(dest.0));
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
                        out.push(release_one(dest.0));
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
            for local in leftover_order(leftover_parent, dying, env_defer) {
                let one = IndexSet::from([local]);
                let keep = die_keep.get(&(bi, si)).unwrap_or(&one);
                if should_release_leftover(keep, n_orig, local)
                    && leftover_env_ok(local, &tokens, env_defer)
                {
                    out.extend(release_and_null(local));
                } else {
                    out.push(null_local(local));
                }
                tokens[local as usize] = false;
                changed = true;
            }
        }

        for local in leftover_order(
            leftover_parent,
            analysis.end_release[bi].iter().copied(),
            env_defer,
        ) {
            if dest_holds_token(&tokens, local) {
                let rel = should_release_leftover(&end_keep[bi], n_orig, local)
                    && leftover_env_ok(local, &tokens, env_defer);
                // A suspended frame can be cancelled before its destination is overwritten
                // by resume. SCCP already treats Await's destination as an unknown definition.
                if rel {
                    out.extend(release_and_null(local));
                } else {
                    out.push(null_local(local));
                }
                tokens[local as usize] = false;
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
    changed
}
