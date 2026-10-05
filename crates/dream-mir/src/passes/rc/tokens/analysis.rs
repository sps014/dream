use super::super::lifetime::call_args_kept_across_await;

use super::super::lifetime::stmt_borrow;

use super::super::is_borrowed_copy;
use super::super::lifetime::StmtBorrow;
use super::super::liveness;
use super::super::liveness::live_after_stmt;
use super::super::liveness::stmt_reads_local;
use super::super::rvalue_reads_local;
use super::super::uniqueness::collect_container_moves;
use super::aliases::leftover_alias_parent;
use super::calls::move_source;
use super::calls::take_owned_arg_locals;
use super::destroy::cursor_riders;
use super::destroy::rc_snapshots_of;
use super::destroy::DestroySite;
use super::flow::join_tokens;
use super::flow::join_unique;
use super::flow::transfer_block;
use super::flow::TokenFlow;
use super::locals::assigns_local;
use super::locals::is_owned_local;
use super::locals::rc_op_on_local;
use super::locals::source_line_end;
use super::locals::take_param_set;
use crate::MirFunction;
use crate::Place;
use crate::Statement;
use crate::Terminator;
use dream_types::DefId;
use dream_types::TypeInterner;
use indexmap::IndexSet;
use std::collections::BTreeSet;

pub(crate) struct TokenAnalysis {
    pub assign_move: IndexSet<(usize, usize)>,
    pub sink_move: IndexSet<(usize, usize, u32)>,
    pub die_after: IndexSet<(usize, usize, u32)>,
    pub start_release: Vec<BTreeSet<u32>>,
    pub end_release: Vec<BTreeSet<u32>>,
    pub token_in: Vec<Vec<bool>>,
    pub token_out: Vec<Vec<bool>>,
    /// Await dest written at the top of this resume block.
    pub await_resume_dest: Vec<Option<u32>>,
    pub has_await: bool,
}

impl TokenAnalysis {
    pub fn analyze(
        func: &MirFunction,
        interner: &TypeInterner,
        layouts: &dream_hir::LayoutTable,
        holds: &IndexSet<DefId>,
        modref: &super::super::modref::ModRefTable,
        analyses: &mut crate::passes::FunctionAnalyses,
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

        let mut assign_move = IndexSet::new();
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

        let mut sink_move = IndexSet::new();
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

        let mut transferred: IndexSet<(usize, usize, u32)> = IndexSet::new();
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
        let snapshot_locals: IndexSet<u32> = rc_snaps.values().flatten().copied().collect();
        let riders = cursor_riders(func);
        let mut die_after: IndexSet<(usize, usize, u32)> = IndexSet::new();
        let site = DestroySite {
            func,
            interner,
            layouts,
            holds,
            modref,
        };
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
                    if riders.get(&local).is_some_and(|cs| {
                        cs.iter()
                            .any(|&c| live_after_stmt(func, &live_out, bi, si, c))
                    }) {
                        continue;
                    }
                    if !site.allows(stmt, local) {
                        continue;
                    }
                    // The release lands at the end of the source line; a rebind before then would
                    // have it drop the new value.
                    let end = source_line_end(block, si);
                    if block.stmts[si + 1..=end]
                        .iter()
                        .any(|s| assigns_local(s, local))
                    {
                        continue;
                    }
                    die_after.insert((bi, end, local));
                }
            }
        }

        let preds = analyses.predecessors(func);
        let entry = func.entry.0 as usize;
        let natural_loops = analyses.natural_loops(func);
        let loop_headers: IndexSet<usize> = natural_loops
            .iter()
            .map(|lp| lp.header.0 as usize)
            .collect();
        let loop_bodies: Vec<IndexSet<usize>> = natural_loops
            .iter()
            .map(|lp| lp.body.iter().map(|b| b.0 as usize).collect())
            .collect();
        let loop_assigns: Vec<IndexSet<u32>> = natural_loops
            .iter()
            .map(|lp| {
                let mut asg = IndexSet::new();
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
        TokenAnalysis {
            assign_move,
            sink_move,
            die_after,
            start_release,
            end_release,
            token_in,
            token_out,
            await_resume_dest,
            has_await,
        }
    }
}
