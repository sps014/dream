//! Compile-time ownership tokens for RC locals.
//!
//! Each owned reference local holds at most one token (the +1 count). Tokens move on last-use
//! assign/sink, stay put on `borrow`, and die at last-use destroy, join balancing, or return.
//! This is CFG dataflow, not ownership-SSA.

use super::lifetime::{call_args_kept_across_await, may_die_after, stmt_borrow, StmtBorrow};
use super::liveness::{self, live_after_stmt, live_in_of, stmt_reads_local};
use super::uniqueness::{apply_stmt_unique, collect_container_moves, meet_unique};
use super::{is_borrowed_copy, is_pure_rvalue, rvalue_reads_local};
use crate::passes::cfg;
use crate::{Const, Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefId, TyKind, TypeInterner};
use std::collections::{BTreeSet, HashMap, HashSet};

/// Owned-RC locals (not cursors, not borrow params). Take-params are owned.
pub(crate) fn is_owned_local(func: &MirFunction, interner: &TypeInterner, local: u32) -> bool {
    let i = local as usize;
    if i >= func.locals.len() {
        return false;
    }
    let d = &func.locals[i];
    if !interner.is_rc_tracked(d.ty) || d.is_cursor {
        return false;
    }
    let is_param = func.params.iter().any(|p| p.0 == local);
    !is_param || d.is_take
}

pub(crate) fn take_param_set(func: &MirFunction) -> HashSet<u32> {
    func.params
        .iter()
        .copied()
        .filter(|p| func.locals[p.0 as usize].is_take)
        .map(|p| p.0)
        .collect()
}

/// Rebind of an owned dest whose RHS may observe the old pointer (`x = f(x)`, `New`, calls).
/// Lower as `tmp = rhs; Release(x); x = tmp` so the call cannot UAF.
/// Concat / ConcatInt only read their operands; native C reuses `dest` in place when unique.
pub(crate) fn needs_rebind_temp(rvalue: &Rvalue, dest: u32) -> bool {
    if rvalue_reads_local(rvalue, dest) {
        return true;
    }
    if matches!(rvalue, Rvalue::Concat(_) | Rvalue::ConcatInt { .. }) {
        return false;
    }
    !is_pure_rvalue(rvalue)
}

pub(crate) struct TokenAnalysis {
    pub assign_move: HashSet<(usize, usize)>,
    pub sink_move: HashSet<(usize, usize, u32)>,
    pub die_after: HashSet<(usize, usize, u32)>,
    pub start_release: Vec<BTreeSet<u32>>,
    pub end_release: Vec<BTreeSet<u32>>,
    pub token_in: Vec<Vec<bool>>,
    pub token_out: Vec<Vec<bool>>,
    pub unique_in: Vec<Vec<bool>>,
    /// Unique token on this block, Shared on a successor that still holds it: Retain before the join.
    pub share_at_end: Vec<BTreeSet<u32>>,
    /// Await dest written at the top of this resume block.
    pub await_resume_dest: Vec<Option<u32>>,
    pub has_await: bool,
}

struct TokenFlow<'a> {
    func: &'a MirFunction,
    interner: &'a TypeInterner,
    is_owned: &'a dyn Fn(u32) -> bool,
    take_params: &'a HashSet<u32>,
    assign_move: &'a HashSet<(usize, usize)>,
    sink_move: &'a HashSet<(usize, usize, u32)>,
    die_after: &'a HashSet<(usize, usize, u32)>,
    live_out: &'a [HashSet<u32>],
    preds: &'a [Vec<crate::BlockId>],
    entry: usize,
    loop_headers: &'a HashSet<usize>,
    loop_bodies: &'a [HashSet<usize>],
    loop_assigns: &'a [HashSet<u32>],
    await_resume_dest: &'a [Option<u32>],
    holds: &'a HashSet<DefId>,
    alias_parent: &'a HashMap<u32, u32>,
    order_parent: &'a HashMap<u32, u32>,
}

impl TokenAnalysis {
    pub fn analyze(
        func: &MirFunction,
        interner: &TypeInterner,
        layouts: &dream_hir::LayoutTable,
        holds: &HashSet<DefId>,
    ) -> TokenAnalysis {
        let n = func.blocks.len();
        let nloc = func.locals.len();
        let live_out = liveness::live_out(func);
        let take_params = take_param_set(func);
        let has_await = func
            .blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Await { .. }));
        let is_owned = |l: u32| is_owned_local(func, interner, l);

        let mut assign_move = HashSet::new();
        for (bi, block) in func.blocks.iter().enumerate() {
            for (si, stmt) in block.stmts.iter().enumerate() {
                let Statement::Assign(Place::Local(dest), rvalue) = stmt else {
                    continue;
                };
                if !is_owned(dest.0) || !is_borrowed_copy(rvalue, interner) {
                    continue;
                }
                if rvalue_reads_local(rvalue, dest.0) {
                    continue;
                }
                let Some(src) = move_source(rvalue, &is_owned) else {
                    continue;
                };
                if !live_after_stmt(func, &live_out, bi, si, src.0) {
                    assign_move.insert((bi, si));
                }
            }
        }

        let mut sink_move = HashSet::new();
        for (bi, block) in func.blocks.iter().enumerate() {
            for (si, stmt) in block.stmts.iter().enumerate() {
                if stmt_borrow(stmt, holds) == StmtBorrow::Held {
                    continue;
                }
                for local in take_owned_arg_locals(stmt, &is_owned) {
                    if !live_after_stmt(func, &live_out, bi, si, local) {
                        sink_move.insert((bi, si, local));
                    }
                }
            }
        }

        let mut transferred: HashSet<(usize, usize, u32)> = HashSet::new();
        for &(bi, si) in &assign_move {
            if let Statement::Assign(_, rvalue) = &func.blocks[bi].stmts[si] {
                if let Some(src) = move_source(rvalue, &is_owned) {
                    transferred.insert((bi, si, src.0));
                }
            }
        }
        collect_container_moves(func, interner, &live_out, is_owned, layouts, &mut sink_move);
        transferred.extend(sink_move.iter().copied());

        // Last-use destroy only at leftover (token_out / Return), Print, and a primitive field
        // load of the owned local (`last_use_destroy`: drop after `println(x.id)`). Destroying
        // after an arbitrary last *read* (RC field/index, Call, union payload) UAFs cursors or
        // last-refs a value still stored in the parent. Sinks are already in `transferred`.
        let rc_snaps = rc_snapshots_of(func, interner);
        let snapshot_locals: HashSet<u32> = rc_snaps.values().flatten().copied().collect();
        let mut die_after: HashSet<(usize, usize, u32)> = HashSet::new();
        for (bi, block) in func.blocks.iter().enumerate() {
            let kept_await = call_args_kept_across_await(block, nloc);
            for (si, stmt) in block.stmts.iter().enumerate() {
                for local in 0..nloc as u32 {
                    if !is_owned(local) || take_params.contains(&local) {
                        continue;
                    }
                    if snapshot_locals.contains(&local) {
                        continue;
                    }
                    if transferred.contains(&(bi, si, local)) || rc_op_on_local(stmt, local) {
                        continue;
                    }
                    if kept_await.contains(&local) {
                        continue;
                    }
                    if live_after_stmt(func, &live_out, bi, si, local) {
                        continue;
                    }
                    // Do not unique-destroy on `x = rhs` merely because `x` is unread before a
                    // later rebind (`held = make_adder(7); mid = …; held = make_adder(0)`).
                    if assigns_local(stmt, local) {
                        continue;
                    }
                    if !stmt_reads_local(stmt, local) {
                        continue;
                    }
                    if rc_snaps.get(&local).is_some_and(|ds| {
                        ds.iter()
                            .any(|&d| live_after_stmt(func, &live_out, bi, si, d))
                    }) {
                        continue;
                    }
                    if !last_use_destroy_site(stmt, local, func, interner, holds) {
                        continue;
                    }
                    die_after.insert((bi, source_line_end(block, si), local));
                }
            }
        }

        let preds = cfg::predecessors(func);
        let entry = func.entry.0 as usize;
        let natural_loops = cfg::natural_loops(func);
        let loop_headers: HashSet<usize> = natural_loops
            .iter()
            .map(|lp| lp.header.0 as usize)
            .collect();
        let loop_bodies: Vec<HashSet<usize>> = natural_loops
            .iter()
            .map(|lp| lp.body.iter().map(|b| b.0 as usize).collect())
            .collect();
        let loop_assigns: Vec<HashSet<u32>> = natural_loops
            .iter()
            .map(|lp| {
                let mut asg = HashSet::new();
                for b in &lp.body {
                    let block = &func.blocks[b.0 as usize];
                    for stmt in &block.stmts {
                        if let Statement::Assign(Place::Local(d), _) = stmt {
                            asg.insert(d.0);
                        }
                    }
                    // An `Await` binds its result into `dest`, so a loop-carried await dest is
                    // reassigned every iteration like any other in-loop assignment.
                    if let Terminator::Await { dest: Some(d), .. } = &block.terminator {
                        asg.insert(d.0);
                    }
                }
                asg
            })
            .collect();
        let mut await_resume_dest = vec![None; n];
        for block in &func.blocks {
            if let Terminator::Await {
                dest: Some(d),
                resume,
                ..
            } = &block.terminator
            {
                await_resume_dest[resume.0 as usize] = Some(d.0);
            }
        }
        let mut token_in = vec![vec![false; nloc]; n];
        let mut token_out: Vec<Vec<Option<bool>>> = vec![vec![None; nloc]; n];
        let mut unique_in = vec![vec![false; nloc]; n];
        let mut unique_out: Vec<Vec<Option<bool>>> = vec![vec![None; nloc]; n];

        let alias_parent = leftover_alias_parent(func, interner, false);
        let order_parent = leftover_alias_parent(func, interner, true);
        let flow = TokenFlow {
            func,
            interner,
            is_owned: &is_owned,
            take_params: &take_params,
            assign_move: &assign_move,
            sink_move: &sink_move,
            die_after: &die_after,
            live_out: &live_out,
            preds: &preds,
            entry,
            loop_headers: &loop_headers,
            loop_bodies: &loop_bodies,
            loop_assigns: &loop_assigns,
            await_resume_dest: &await_resume_dest,
            holds,
            alias_parent: &alias_parent,
            order_parent: &order_parent,
        };
        let mut start_release = vec![BTreeSet::new(); n];
        let mut end_release = vec![BTreeSet::new(); n];
        for _ in 0..64 {
            let mut changed = false;
            for bi in 0..n {
                let inn = join_tokens(&flow, &token_out, bi);
                if inn != token_in[bi] {
                    token_in[bi] = inn;
                    changed = true;
                }
                let uin = join_unique(&flow, &unique_out, &token_out, bi, &token_in[bi]);
                if uin != unique_in[bi] {
                    unique_in[bi] = uin;
                    changed = true;
                }
                let (out, uout, start, end) =
                    transfer_block(&flow, &token_in[bi], &unique_in[bi], &token_out, bi);
                let out_opt: Vec<Option<bool>> = out.into_iter().map(Some).collect();
                let uout_opt: Vec<Option<bool>> = uout.into_iter().map(Some).collect();
                if out_opt != token_out[bi] {
                    token_out[bi] = out_opt;
                    changed = true;
                }
                if uout_opt != unique_out[bi] {
                    unique_out[bi] = uout_opt;
                    changed = true;
                }
                if start != start_release[bi] {
                    start_release[bi] = start;
                    changed = true;
                }
                if end != end_release[bi] {
                    end_release[bi] = end;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let token_out: Vec<Vec<bool>> = token_out
            .into_iter()
            .map(|row| row.into_iter().map(|t| t.unwrap_or(false)).collect())
            .collect();
        let unique_out: Vec<Vec<bool>> = unique_out
            .into_iter()
            .map(|row| row.into_iter().map(|t| t.unwrap_or(false)).collect())
            .collect();

        let mut share_at_end = vec![BTreeSet::new(); n];
        for bi in 0..n {
            for succ in func.blocks[bi].terminator.successors() {
                let s = succ.0 as usize;
                // A loop header is Shared because the back-edge meets the entry edge.
                // That is one token going around, not a second owner — Retain here leaks
                // (StringBuilder across stringify loops, take params in add_all / serve_loop).
                if loop_headers.contains(&s) {
                    continue;
                }
                for local in 0..nloc {
                    if token_out[bi][local]
                        && unique_out[bi][local]
                        && token_in[s][local]
                        && !unique_in[s][local]
                    {
                        // Phi dest: this arm *assigned* `local` (e.g. `unwrap_or` `None => fallback`).
                        // The other arm's Shared is a different value, not a second owner of this
                        // object. Retain would leak the moved fallback (`Option.unwrap_or`).
                        if func.blocks[bi]
                            .stmts
                            .iter()
                            .any(|stmt| assigns_local(stmt, local as u32))
                        {
                            continue;
                        }
                        share_at_end[bi].insert(local as u32);
                    }
                }
            }
        }

        TokenAnalysis {
            assign_move,
            sink_move,
            die_after,
            start_release,
            end_release,
            token_in,
            token_out,
            unique_in,
            share_at_end,
            await_resume_dest,
            has_await,
        }
    }
}

fn join_tokens(flow: &TokenFlow<'_>, token_out: &[Vec<Option<bool>>], bi: usize) -> Vec<bool> {
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

fn join_unique(
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

fn pred_tokens_unbalanced(
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
fn keep_unread_across_loop(flow: &TokenFlow<'_>, bi: usize, local: u32) -> bool {
    flow.loop_bodies
        .iter()
        .zip(flow.loop_assigns.iter())
        .any(|(body, asg)| body.contains(&bi) && !asg.contains(&local))
}

fn transfer_block(
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
    if let Some(d) = flow.await_resume_dest[bi] {
        if (flow.is_owned)(d) {
            tokens[d as usize] = true;
            unique[d as usize] = true;
        }
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
    {
        if dest != &Some(*l)
            && (flow.is_owned)(l.0)
            && tokens[l.0 as usize]
            && !live_in_of(flow.func, flow.live_out, resume.0 as usize).contains(&l.0)
        {
            tokens[l.0 as usize] = false;
            unique[l.0 as usize] = false;
        }
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

pub(crate) fn apply_stmt_tokens(
    stmt: &Statement,
    interner: &TypeInterner,
    is_owned: &dyn Fn(u32) -> bool,
    assign_is_move: bool,
    sink_is_move: impl Fn(u32) -> bool,
    tokens: &mut [bool],
) {
    if let Statement::Assign(Place::Local(dest), rvalue) = stmt {
        if is_owned(dest.0) {
            let self_ref = rvalue_reads_local(rvalue, dest.0);
            if !self_ref {
                tokens[dest.0 as usize] = false;
                if is_borrowed_copy(rvalue, interner) {
                    if let Some(src) = move_source(rvalue, is_owned) {
                        if assign_is_move {
                            tokens[src.0 as usize] = false;
                        }
                    }
                    tokens[dest.0 as usize] = true;
                } else {
                    tokens[dest.0 as usize] = true;
                }
            }
        }
    }
    for local in take_owned_arg_locals(stmt, is_owned) {
        if sink_is_move(local) {
            tokens[local as usize] = false;
        }
    }
    for src in super::uniqueness::container_move_locals(stmt) {
        if sink_is_move(src) {
            tokens[src as usize] = false;
        }
    }
}

pub(crate) fn dest_holds_token(tokens: &[bool], dest: u32) -> bool {
    tokens.get(dest as usize).copied().unwrap_or(false)
}

fn reads_local_in_block(block: &crate::BasicBlock, local: u32) -> bool {
    block.stmts.iter().any(|s| stmt_reads_local(s, local))
}

pub(crate) fn terminator_reads_local(term: &Terminator, local: u32) -> bool {
    let mut live = HashSet::new();
    match term {
        Terminator::If { cond, .. } => add_op(cond, &mut live),
        Terminator::Switch { value, .. } => add_op(value, &mut live),
        Terminator::Return(Some(o)) | Terminator::AsyncComplete(Some(o)) => add_op(o, &mut live),
        Terminator::TailCall { args, .. } => args.iter().for_each(|a| add_op(a, &mut live)),
        Terminator::Await { future, .. } => add_op(future, &mut live),
        _ => {}
    }
    live.contains(&local)
}

fn add_op(op: &Operand, live: &mut HashSet<u32>) {
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
                add_op(index, live);
            }
            Place::Global(_) => {}
        }
    }
}

/// Copy / niche-union / field / index / `unwrap_or`: dest aliases `src`.
///
/// `calls`: also `dest = f(src, …)` so leftover_order releases a `JsonValue.get` dest before
/// leftover of `this`. Do not use that edge for leftover_waits: `r.text()` / concat dests would
/// wait for a still-live receiver and leak (`webapi_basic`).
pub(crate) fn leftover_alias_parent(
    func: &MirFunction,
    interner: &TypeInterner,
    calls: bool,
) -> HashMap<u32, u32> {
    let mut parent = HashMap::new();
    let ty = |l: u32| func.locals.get(l as usize).map(|d| d.ty);
    for block in &func.blocks {
        for stmt in &block.stmts {
            match stmt {
                Statement::Assign(
                    Place::Local(dest),
                    Rvalue::Use(Operand::Copy(Place::Local(src))),
                ) if calls || ty(dest.0) == ty(src.0) => {
                    parent.entry(dest.0).or_insert(src.0);
                }
                Statement::Assign(
                    Place::Local(dest),
                    Rvalue::Cast(Operand::Copy(Place::Local(src)), _, _),
                ) => {
                    // `funcbox_new(idx, env as int)`: leftover the box before leftover of the
                    // env array so typed `release_array_*` still sees the last +1.
                    parent.entry(dest.0).or_insert(src.0);
                }
                Statement::Assign(
                    Place::Local(dest),
                    Rvalue::Use(Operand::Copy(Place::Field { base, .. })),
                ) => {
                    parent.entry(dest.0).or_insert(base.0);
                }
                Statement::Assign(
                    Place::Local(dest),
                    Rvalue::Use(Operand::Copy(Place::Index { base, .. })),
                ) => {
                    parent.entry(dest.0).or_insert(base.0);
                }
                Statement::Assign(
                    Place::Local(dest),
                    Rvalue::UnionField {
                        base: Operand::Copy(Place::Local(src)),
                        ..
                    },
                ) => {
                    parent.entry(dest.0).or_insert(src.0);
                }
                Statement::Assign(Place::Local(dest), Rvalue::Call { callee, args, .. })
                    if !args.is_empty() =>
                {
                    let Operand::Copy(Place::Local(src)) = &args[0] else {
                        continue;
                    };
                    if dest.0 != src.0 {
                        // Borrow `this` methods (`JsonValue.get`): leftover dest waits for `this`
                        // and leftover_order releases dest first. Take first-arg (`concat`) must
                        // not wait — the dest is a new object and the first arg may stay live.
                        let borrow_this = callee.take_params.first() != Some(&true);
                        if calls || borrow_this {
                            parent.entry(dest.0).or_insert(src.0);
                        }
                    }
                    if args.len() == 2 {
                        let Operand::Copy(Place::Local(fb)) = &args[1] else {
                            continue;
                        };
                        if ty(dest.0) == ty(fb.0) {
                            parent.entry(dest.0).or_insert(src.0);
                        }
                    }
                    // `funcbox_new(idx, env)`: dest is a fun, arg0 is int. Leftover the box before
                    // leftover of the env array so typed array release still sees the last +1.
                    if calls && args.len() >= 2 && ty(dest.0) != ty(src.0) {
                        if let Operand::Copy(Place::Local(env)) = &args[1] {
                            parent.insert(dest.0, env.0);
                        }
                    }
                    // `Next(handler)`: dest is the cell, last arg is the funcbox. Leftover of
                    // `Next` must count as covering that box so env leftover still 2→1 then last-drop.
                    if calls {
                        if let Some(Operand::Copy(Place::Local(last))) = args.last() {
                            if matches!(interner.kind(func.local_ty(*last)), TyKind::Func(..)) {
                                parent.insert(dest.0, last.0);
                            }
                        }
                    }
                }
                Statement::Assign(Place::Local(dest), Rvalue::New { args, .. }) if calls => {
                    for arg in args.iter().rev() {
                        let Operand::Copy(Place::Local(src)) = arg else {
                            continue;
                        };
                        if dest.0 != src.0 {
                            parent.entry(dest.0).or_insert(src.0);
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    parent
}

fn leftover_waits_for_live_parent(
    parent: &HashMap<u32, u32>,
    local: u32,
    live: &HashSet<u32>,
) -> bool {
    let mut x = local;
    let mut seen = HashSet::new();
    while seen.insert(x) {
        let Some(&p) = parent.get(&x) else {
            return false;
        };
        if live.contains(&p) && p != local {
            return true;
        }
        x = p;
    }
    false
}

/// Locals in `ids` that should `Release`. Dest leftover is delayed until the parent leftover
/// site (`order_parent` skip in `transfer_block`) so extras and `this` share one batch;
/// skip-coalesce here last-refs a map occupant (`union_json`) or leaks extras (`json_parse`).
pub(crate) fn leftover_keep(
    _func: &MirFunction,
    ids: impl IntoIterator<Item = u32>,
) -> HashSet<u32> {
    ids.into_iter().collect()
}

/// `(funcbox dest, env RC root)` for each `funcbox_new` (env may be an `int` pun of an `object[]`).
pub(crate) fn funcbox_env_pairs(func: &MirFunction, interner: &TypeInterner) -> Vec<(u32, u32)> {
    let parent = leftover_alias_parent(func, interner, true);
    let mut pairs = Vec::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            let (dest, args) = match stmt {
                Statement::Assign(Place::Local(dest), Rvalue::Call { args, .. }) => (dest, args),
                _ => continue,
            };
            if args.len() < 2 {
                continue;
            }
            if !matches!(interner.kind(func.local_ty(*dest)), TyKind::Func(..)) {
                continue;
            }
            let Operand::Copy(Place::Local(env)) = &args[1] else {
                continue;
            };
            let mut x = env.0;
            let mut seen = HashSet::new();
            loop {
                if !seen.insert(x) {
                    break;
                }
                if interner.is_rc_tracked(func.local_ty(Local(x))) {
                    pairs.push((dest.0, x));
                    break;
                }
                match parent.get(&x) {
                    Some(&p) => x = p,
                    None => break,
                }
            }
        }
    }
    pairs
}

pub(crate) fn funcbox_env_rc_roots(func: &MirFunction, interner: &TypeInterner) -> HashSet<u32> {
    funcbox_env_pairs(func, interner)
        .into_iter()
        .map(|(_, env)| env)
        .collect()
}

/// Child alias dests before parents so leftover Release of an extra-retain occupant runs while
/// the container still holds +1 (parent-first last-refs the map slot under the dest).
pub(crate) fn leftover_order(
    parent: &HashMap<u32, u32>,
    ids: impl IntoIterator<Item = u32>,
    defer: &HashSet<u32>,
) -> Vec<u32> {
    let ids: Vec<u32> = ids.into_iter().collect();
    let set: HashSet<u32> = ids.iter().copied().collect();
    let mut indeg: HashMap<u32, u32> = ids.iter().map(|&x| (x, 0)).collect();
    let mut edge: HashMap<u32, u32> = HashMap::new();
    for &d in &ids {
        let mut x = d;
        let mut seen = HashSet::new();
        while seen.insert(x) {
            let Some(&p) = parent.get(&x) else {
                break;
            };
            if set.contains(&p) {
                edge.insert(d, p);
                if let Some(n) = indeg.get_mut(&p) {
                    *n += 1;
                }
                break;
            }
            x = p;
        }
    }
    let mut ready: BTreeSet<(u8, u32)> = indeg
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(&x, _)| {
            // Children (have a parent) before roots so leftover of `this` (often local 0)
            // cannot last-ref a map under still-live get/unwrap dests.
            let child = if parent.contains_key(&x) { 0 } else { 1 };
            (child, x)
        })
        .collect();
    let mut out = Vec::with_capacity(ids.len());
    let mut left = set;
    while let Some((_, d)) = ready.pop_first() {
        if !left.remove(&d) {
            continue;
        }
        out.push(d);
        if let Some(&p) = edge.get(&d) {
            if let Some(n) = indeg.get_mut(&p) {
                *n = n.saturating_sub(1);
                if *n == 0 && left.contains(&p) {
                    let child = if parent.contains_key(&p) { 0 } else { 1 };
                    ready.insert((child, p));
                }
            }
        }
    }
    let mut rest: Vec<u32> = left.into_iter().collect();
    rest.sort_unstable();
    out.extend(rest);
    if defer.is_empty() {
        return out;
    }
    let mut first = Vec::with_capacity(out.len());
    let mut last = Vec::new();
    for x in out {
        if defer.contains(&x) {
            last.push(x);
        } else {
            first.push(x);
        }
    }
    first.extend(last);
    first
}

pub(crate) fn null_local(local: u32) -> Statement {
    Statement::Assign(
        Place::Local(Local(local)),
        Rvalue::Use(Operand::Const(Const::Null)),
    )
}

pub(crate) fn release_and_null(local: u32, unique: bool) -> [Statement; 2] {
    let op = Operand::Copy(Place::Local(Local(local)));
    [
        if unique {
            Statement::ReleaseUnique(op)
        } else {
            Statement::Release(op)
        },
        Statement::Assign(
            Place::Local(Local(local)),
            Rvalue::Use(Operand::Const(Const::Null)),
        ),
    ]
}

pub(crate) fn rc_op_on_local(stmt: &Statement, local: u32) -> bool {
    match stmt {
        Statement::Retain(Operand::Copy(Place::Local(l)))
        | Statement::Release(Operand::Copy(Place::Local(l)))
        | Statement::ReleaseUnique(Operand::Copy(Place::Local(l))) => l.0 == local,
        Statement::Assign(Place::Local(l), Rvalue::Use(Operand::Const(Const::Null))) => {
            l.0 == local
        }
        _ => false,
    }
}

pub(crate) fn assigns_local(stmt: &Statement, local: u32) -> bool {
    matches!(stmt, Statement::Assign(Place::Local(l), _) if l.0 == local)
}

fn rc_snapshots_of(func: &MirFunction, interner: &TypeInterner) -> HashMap<u32, Vec<u32>> {
    let mut m: HashMap<u32, Vec<u32>> = HashMap::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            let Statement::Assign(Place::Local(dest), rv) = stmt else {
                continue;
            };
            if !interner.is_rc_tracked(func.locals[dest.0 as usize].ty) {
                continue;
            }
            let base = match rv {
                Rvalue::Use(Operand::Copy(Place::Field { base, .. }))
                | Rvalue::Use(Operand::Copy(Place::Index { base, .. }))
                | Rvalue::Cast(Operand::Copy(Place::Field { base, .. }), _, _)
                | Rvalue::Cast(Operand::Copy(Place::Index { base, .. }), _, _) => base.0,
                Rvalue::UnionField {
                    base: Operand::Copy(Place::Local(b)),
                    ..
                } => b.0,
                _ => continue,
            };
            m.entry(base).or_default().push(dest.0);
        }
    }
    m
}

fn last_use_destroy_site(
    stmt: &Statement,
    local: u32,
    func: &MirFunction,
    interner: &TypeInterner,
    holds: &HashSet<DefId>,
) -> bool {
    if !may_die_after(stmt, holds) {
        return false;
    }
    match stmt {
        Statement::Print { .. } => true,
        Statement::Assign(Place::Local(dest), rv) => match rv {
            Rvalue::Use(Operand::Copy(Place::Field { base, .. })) if base.0 == local => func
                .locals
                .get(dest.0 as usize)
                .is_some_and(|d| !interner.is_rc_tracked(d.ty)),
            _ => false,
        },
        _ => false,
    }
}

pub(crate) fn source_line_end(block: &crate::BasicBlock, si: usize) -> usize {
    let mut end = si;
    for (j, s) in block.stmts.iter().enumerate().skip(si + 1) {
        if matches!(s, Statement::SourceLine(_)) {
            break;
        }
        end = j;
    }
    end
}

pub(crate) fn move_source(rvalue: &Rvalue, is_owned_ref: &dyn Fn(u32) -> bool) -> Option<Local> {
    match rvalue {
        Rvalue::Use(Operand::Copy(Place::Local(src))) if is_owned_ref(src.0) => Some(*src),
        _ => None,
    }
}

pub(crate) fn sink_call_args(stmt: &Statement) -> Option<(Vec<bool>, &[Operand])> {
    match stmt {
        Statement::Call { callee, args } => Some((callee.take_params.clone(), args)),
        Statement::Assign(_, Rvalue::Call { callee, args, .. }) => {
            Some((callee.take_params.clone(), args))
        }
        // A constructor arg is a sink unless the ctor declares that parameter `borrow`/`ref`, in
        // which case the ctor body's own field store does the retain. Flags that don't line up with
        // the args mean the ctor's declaration wasn't resolvable, and unknown must retain —
        // under-retaining frees a live object.
        Statement::Assign(
            _,
            Rvalue::New {
                ctor: Some(c),
                args,
                ..
            },
        ) => {
            let flags = if c.take_params.len() == args.len() {
                c.take_params.clone()
            } else {
                vec![true; args.len()]
            };
            Some((flags, args))
        }
        Statement::IndirectCall { args, .. } => Some((vec![true; args.len()], args)),
        Statement::Assign(_, Rvalue::IndirectCall { args, .. }) => {
            Some((vec![true; args.len()], args))
        }
        Statement::InterfaceCall { args, .. } => Some((vec![true; args.len()], args)),
        Statement::Assign(_, Rvalue::InterfaceCall { args, .. }) => {
            Some((vec![true; args.len()], args))
        }
        _ => None,
    }
}

pub(crate) fn take_arg_effects(
    stmt: &Statement,
    is_owned_ref: &dyn Fn(u32) -> bool,
    local_is_ref: &[bool],
    is_move: impl Fn(u32) -> bool,
) -> (Vec<Statement>, Vec<Statement>) {
    let Some((take_params, args)) = sink_call_args(stmt) else {
        return (Vec::new(), Vec::new());
    };
    // Fun-value calls (`IndirectCall`) have no per-param ABI on `TyKind::Func`. Treating every
    // arg as sink retains borrow params of `Middleware.invoke` into a borrow handler (`webapi_basic`).
    let fun_value = matches!(
        stmt,
        Statement::IndirectCall { .. } | Statement::Assign(_, Rvalue::IndirectCall { .. })
    );
    let mut retains = Vec::new();
    let mut nulls = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if !take_params.get(i).copied().unwrap_or(false) {
            continue;
        }
        match arg {
            Operand::Copy(Place::Local(l))
                if local_is_ref.get(l.0 as usize).copied().unwrap_or(false) =>
            {
                if fun_value && !is_owned_ref(l.0) {
                    continue;
                }
                if is_owned_ref(l.0) && is_move(l.0) {
                    nulls.push(Statement::Assign(
                        Place::Local(*l),
                        Rvalue::Use(Operand::Const(Const::Null)),
                    ));
                } else {
                    retains.push(Statement::Retain(Operand::Copy(Place::Local(*l))));
                }
            }
            Operand::Copy(Place::Field { .. })
            | Operand::Copy(Place::Index { .. })
            | Operand::Const(Const::Str(_)) => {
                retains.push(Statement::Retain(arg.clone()));
            }
            _ => {}
        }
    }
    (retains, nulls)
}

pub(crate) fn take_owned_arg_locals(
    stmt: &Statement,
    is_owned_ref: &dyn Fn(u32) -> bool,
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
            if is_owned_ref(l.0) {
                out.push(l.0);
            }
        }
    }
    out
}

/// Borrowed or taken call arguments may be retained by the callee; Unique last-use destroy is unsound.
pub(crate) fn call_escape_locals(stmt: &Statement, is_owned_ref: &dyn Fn(u32) -> bool) -> Vec<u32> {
    let Some((_, args)) = sink_call_args(stmt) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for arg in args {
        if let Operand::Copy(Place::Local(l)) = arg {
            if is_owned_ref(l.0) {
                out.push(l.0);
            }
        }
    }
    out
}
