use super::super::lifetime::call_args_kept_across_await;

use super::super::lifetime::stmt_borrow;

use super::super::lifetime::StmtBorrow;
use super::super::liveness::live_in_of;
use super::super::liveness::stmt_reads_local;
use super::super::uniqueness::apply_stmt_unique;
use super::super::uniqueness::meet_unique;
use super::aliases::leftover_waits_for_live_parent;
use super::locals::reads_local_in_block;
use super::locals::terminator_reads_local;
use super::statements::apply_stmt_tokens;
use crate::MirFunction;
use crate::Operand;
use crate::Place;
use crate::Terminator;
use dream_types::DefId;
use dream_types::TypeInterner;
use indexmap::IndexMap;
use indexmap::IndexSet;
use std::collections::BTreeSet;

pub(super) struct TokenFlow<'a> {
    pub(super) func: &'a MirFunction,
    pub(super) interner: &'a TypeInterner,
    pub(super) is_owned: &'a dyn Fn(u32) -> bool,
    pub(super) take_params: &'a IndexSet<u32>,
    pub(super) assign_move: &'a IndexSet<(usize, usize)>,
    pub(super) sink_move: &'a IndexSet<(usize, usize, u32)>,
    pub(super) die_after: &'a IndexSet<(usize, usize, u32)>,
    pub(super) live_out: &'a [IndexSet<u32>],
    pub(super) preds: &'a [Vec<crate::BlockId>],
    pub(super) entry: usize,
    pub(super) loop_headers: &'a IndexSet<usize>,
    pub(super) loop_bodies: &'a [IndexSet<usize>],
    pub(super) loop_assigns: &'a [IndexSet<u32>],
    pub(super) await_resume_dest: &'a [Option<u32>],
    pub(super) holds: &'a IndexSet<DefId>,
    pub(super) alias_parent: &'a IndexMap<u32, u32>,
    pub(super) order_parent: &'a IndexMap<u32, u32>,
}

pub(super) fn join_tokens(
    flow: &TokenFlow<'_>,
    token_out: &[Vec<Option<bool>>],
    bi: usize,
) -> Vec<bool> {
    let nloc = flow.func.locals.len();
    let mut inn = vec![false; nloc];
    for local in 0..nloc as u32 {
        if !(flow.is_owned)(local) {
            continue;
        }
        let mut any_owned = false;
        for p in &flow.preds[bi] {
            if token_out[p.0 as usize][local as usize] == Some(true) {
                any_owned = true;
            }
        }
        if bi == flow.entry && flow.take_params.contains(&local) {
            any_owned = true;
        }
        if flow.take_params.contains(&local) {
            inn[local as usize] = any_owned;
            continue;
        }
        // Loop headers of locals live across the loop start Owned so the first pass does not
        // treat an unprocessed back-edge as Empty.
        if flow.loop_headers.contains(&bi) {
            let live_in = live_in_of(flow.func, flow.live_out, bi);
            if live_in.contains(&local) && (flow.is_owned)(local) {
                let any_known_empty = flow.preds[bi]
                    .iter()
                    .any(|p| token_out[p.0 as usize][local as usize] == Some(false));
                let any_known_owned = any_owned;
                inn[local as usize] = any_known_owned || !any_known_empty;
                continue;
            }
        }
        inn[local as usize] = any_owned;
    }
    inn
}

pub(super) fn join_unique(
    flow: &TokenFlow<'_>,
    unique_out: &[Vec<Option<bool>>],
    token_out: &[Vec<Option<bool>>],
    bi: usize,
    token_in: &[bool],
) -> Vec<bool> {
    let nloc = flow.func.locals.len();
    let mut inn = vec![false; nloc];
    for local in 0..nloc as u32 {
        if !token_in[local as usize] || !(flow.is_owned)(local) {
            continue;
        }
        if flow.loop_headers.contains(&bi) {
            let live_in = live_in_of(flow.func, flow.live_out, bi);
            if live_in.contains(&local) {
                let mut all_unique = true;
                let mut saw = false;
                for p in &flow.preds[bi] {
                    if token_out[p.0 as usize][local as usize] == Some(true) {
                        saw = true;
                        all_unique = meet_unique(
                            all_unique,
                            unique_out[p.0 as usize][local as usize] != Some(false),
                        );
                    }
                }
                inn[local as usize] = if saw { all_unique } else { true };
                continue;
            }
        }
        let mut all_unique = true;
        let mut saw = false;
        for p in &flow.preds[bi] {
            if token_out[p.0 as usize][local as usize] == Some(true) {
                saw = true;
                all_unique = meet_unique(
                    all_unique,
                    unique_out[p.0 as usize][local as usize] == Some(true),
                );
            }
        }
        inn[local as usize] = if saw { all_unique } else { true };
    }
    inn
}

pub(super) fn pred_tokens_unbalanced(
    flow: &TokenFlow<'_>,
    token_out: &[Vec<Option<bool>>],
    bi: usize,
    local: u32,
) -> bool {
    let mut saw_owned = false;
    let mut saw_empty = false;
    for p in &flow.preds[bi] {
        match token_out[p.0 as usize][local as usize] {
            Some(true) => saw_owned = true,
            Some(false) => saw_empty = true,
            None => {}
        }
    }
    saw_owned && saw_empty
}

/// Outer locals that are never written in a natural loop stay in scope across it
/// (`warm` in `closure_env_reclaim`). Leftover on the header/back-edge would drop them
/// after a pre-loop `Debug.live_objects` baseline.
pub(super) fn keep_unread_across_loop(flow: &TokenFlow<'_>, bi: usize, local: u32) -> bool {
    flow.loop_bodies
        .iter()
        .zip(flow.loop_assigns.iter())
        .any(|(body, asg)| body.contains(&bi) && !asg.contains(&local))
}

pub(super) fn transfer_block(
    flow: &TokenFlow<'_>,
    token_in: &[bool],
    unique_in: &[bool],
    token_out: &[Vec<Option<bool>>],
    bi: usize,
) -> (Vec<bool>, Vec<bool>, BTreeSet<u32>, BTreeSet<u32>) {
    let mut tokens = token_in.to_vec();
    let mut unique = unique_in.to_vec();
    let block = &flow.func.blocks[bi];
    let live_in = live_in_of(flow.func, flow.live_out, bi);
    let mut start = BTreeSet::new();
    for (local, slot) in tokens.iter_mut().enumerate() {
        let local = local as u32;
        if !*slot || flow.take_params.contains(&local) || !(flow.is_owned)(local) {
            continue;
        }
        if live_in.contains(&local) || flow.live_out[bi].contains(&local) {
            continue;
        }
        if keep_unread_across_loop(flow, bi, local) {
            continue;
        }
        // Dest leftover waits for leftover of its alias parent (same leftover_keep batch).
        // Mid-block leftover of a `JsonValue.get` dest last-refs a map occupant (`union_json`).
        if leftover_waits_for_live_parent(flow.alias_parent, local, &live_in)
            || leftover_waits_for_live_parent(flow.alias_parent, local, &flow.live_out[bi])
            || flow.order_parent.contains_key(&local)
        {
            continue;
        }
        // Return leftover runs after this block's stmts (`print` then drop). A mixed-join
        // start_release would unique-destroy here first (`in_union` dropped Tracked before
        // printing the payload id).
        if matches!(
            block.terminator,
            Terminator::Return(_) | Terminator::AsyncComplete(_)
        ) {
            continue;
        }
        // Only the split edge of a mixed join. A single-pred successor (switch arm) is not a
        // join: releasing here drops a weak-store referent before the arm reads it.
        if !pred_tokens_unbalanced(flow, token_out, bi, local) {
            continue;
        }
        start.insert(local);
        *slot = false;
        unique[local as usize] = false;
    }
    if let Some(d) = flow.await_resume_dest[bi]
        && (flow.is_owned)(d)
    {
        tokens[d as usize] = true;
        unique[d as usize] = true;
    }
    for (si, stmt) in block.stmts.iter().enumerate() {
        apply_stmt_tokens(
            stmt,
            flow.interner,
            flow.is_owned,
            flow.assign_move.contains(&(bi, si)),
            |l| flow.sink_move.contains(&(bi, si, l)),
            &mut tokens,
        );
        apply_stmt_unique(
            stmt,
            flow.interner,
            flow.is_owned,
            flow.assign_move.contains(&(bi, si)),
            |l| flow.sink_move.contains(&(bi, si, l)),
            &mut unique,
        );
        for (local, slot) in tokens.iter_mut().enumerate() {
            if flow.die_after.contains(&(bi, si, local as u32)) {
                *slot = false;
                unique[local] = false;
            }
        }
    }

    if let Terminator::Await {
        future: Operand::Copy(Place::Local(l)),
        dest,
        resume,
    } = &block.terminator
        && dest != &Some(*l)
        && (flow.is_owned)(l.0)
        && tokens[l.0 as usize]
        && !live_in_of(flow.func, flow.live_out, resume.0 as usize).contains(&l.0)
    {
        tokens[l.0 as usize] = false;
        unique[l.0 as usize] = false;
    }

    let await_clobber: Option<u32> = match &block.terminator {
        Terminator::Await {
            future: Operand::Copy(Place::Local(f)),
            dest: Some(d),
            ..
        } if d != f => Some(d.0),
        _ => None,
    };

    let delay_held = matches!(block.terminator, Terminator::Await { .. });
    let mut held_args = call_args_kept_across_await(block, tokens.len());
    if delay_held {
        for stmt in &block.stmts {
            if stmt_borrow(stmt, flow.holds) == StmtBorrow::Held {
                for local in 0..tokens.len() as u32 {
                    if stmt_reads_local(stmt, local) && (flow.is_owned)(local) {
                        held_args.insert(local);
                    }
                }
            }
        }
    }

    let mut end = BTreeSet::new();
    for (local, slot) in tokens.iter_mut().enumerate() {
        let local = local as u32;
        let clobber = await_clobber == Some(local);
        if !*slot || !(flow.is_owned)(local) {
            continue;
        }
        if flow.take_params.contains(&local)
            && !clobber
            && !matches!(
                block.terminator,
                Terminator::Return(_) | Terminator::AsyncComplete(_)
            )
        {
            continue;
        }
        if delay_held && held_args.contains(&local) && !clobber {
            continue;
        }
        if flow.live_out[bi].contains(&local) && !clobber {
            continue;
        }
        if keep_unread_across_loop(flow, bi, local) {
            continue;
        }
        if leftover_waits_for_live_parent(flow.alias_parent, local, &flow.live_out[bi])
            || flow.order_parent.contains_key(&local)
        {
            continue;
        }
        if !clobber && terminator_reads_local(&block.terminator, local) {
            continue;
        }
        if !clobber && reads_local_in_block(block, local) {
            continue;
        }
        end.insert(local);
        *slot = false;
        unique[local as usize] = false;
    }
    (tokens, unique, start, end)
}
