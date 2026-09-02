//! Compile-time ownership tokens for RC locals.
//!
//! Each owned reference local holds at most one token (the +1 count). Tokens move on last-use
//! assign/sink, stay put on `borrow`, and die at last-use destroy, join balancing, or return.
//! This is CFG dataflow, not ownership-SSA.

use super::liveness::{self, live_after_stmt, live_in_of, stmt_reads_local};
use super::uniqueness::{apply_stmt_unique, collect_container_moves, meet_unique};
use super::{is_borrowed_copy, is_pure_rvalue, rvalue_reads_local};
use crate::passes::cfg;
use crate::{Const, Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;
use std::collections::{BTreeSet, HashSet};

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

/// Strings / arrays / funcboxes / unions may be borrowed by a callee that stashes a raw pointer
/// (funcbox env, union spine, group buffer). Do not destroy these until block-end or return —
/// a mid-block `Release` after a hidden borrow is UAF.
pub(crate) fn is_hidden_borrow_ty(func: &MirFunction, interner: &TypeInterner, local: u32) -> bool {
    let ty = func.locals[local as usize].ty;
    ty == interner.string()
        || matches!(
            interner.kind(ty),
            dream_types::TyKind::Array(_)
                | dream_types::TyKind::Func(_, _)
                | dream_types::TyKind::Union(_, _)
        )
}

/// Classes / `js` may destroy at last use; hidden-borrow types wait for block-end or return.
pub(crate) fn is_early_destroy_ty(func: &MirFunction, interner: &TypeInterner, local: u32) -> bool {
    !is_hidden_borrow_ty(func, interner, local)
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
    pub unique_out: Vec<Vec<bool>>,
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
    await_resume_dest: &'a [Option<u32>],
}

impl TokenAnalysis {
    pub fn analyze(
        func: &MirFunction,
        interner: &TypeInterner,
        layouts: &dream_hir::LayoutTable,
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

        let mut die_after: HashSet<(usize, usize, u32)> = HashSet::new();
        for (bi, block) in func.blocks.iter().enumerate() {
            for (si, stmt) in block.stmts.iter().enumerate() {
                for local in 0..nloc as u32 {
                    if !is_owned(local) || take_params.contains(&local) {
                        continue;
                    }
                    if !is_early_destroy_ty(func, interner, local) {
                        continue;
                    }
                    if transferred.contains(&(bi, si, local)) || rc_op_on_local(stmt, local) {
                        continue;
                    }
                    let used = stmt_reads_local(stmt, local);
                    let defined_dead = assigns_local(stmt, local)
                        && !live_after_stmt(func, &live_out, bi, si, local);
                    if !used && !defined_dead {
                        continue;
                    }
                    if used && live_after_stmt(func, &live_out, bi, si, local) {
                        continue;
                    }
                    if defined_dead || (used && !live_after_stmt(func, &live_out, bi, si, local)) {
                        if !allows_early_destroy(stmt) {
                            continue;
                        }
                        die_after.insert((bi, source_line_end(block, si), local));
                    }
                }
            }
        }

        let preds = cfg::predecessors(func);
        let entry = func.entry.0 as usize;
        let loop_headers: HashSet<usize> = cfg::natural_loops(func)
            .iter()
            .map(|lp| lp.header.0 as usize)
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
            await_resume_dest: &await_resume_dest,
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
        let mut unique_out: Vec<Vec<bool>> = unique_out
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
                        share_at_end[bi].insert(local as u32);
                    }
                }
            }
            for &local in &share_at_end[bi] {
                unique_out[bi][local as usize] = false;
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
            unique_out,
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
        if is_hidden_borrow_ty(flow.func, flow.interner, local) {
            continue;
        }
        if live_in.contains(&local) || flow.live_out[bi].contains(&local) {
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

    let mut end = BTreeSet::new();
    for (local, slot) in tokens.iter_mut().enumerate() {
        let local = local as u32;
        let clobber = await_clobber == Some(local);
        if !*slot || !(flow.is_owned)(local) {
            continue;
        }
        if flow.take_params.contains(&local) && !clobber {
            continue;
        }
        if is_hidden_borrow_ty(flow.func, flow.interner, local) && !clobber {
            continue;
        }
        if flow.live_out[bi].contains(&local) && !clobber {
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

fn terminator_reads_local(term: &Terminator, local: u32) -> bool {
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

pub(crate) fn allows_early_destroy(stmt: &Statement) -> bool {
    match stmt {
        Statement::Print { .. } | Statement::JsCall { .. } => true,
        Statement::Assign(_, rv) => matches!(
            rv,
            Rvalue::Use(_)
                | Rvalue::Select { .. }
                | Rvalue::Unary(_, _)
                | Rvalue::Binary(_, _, _)
                | Rvalue::UnionField { .. }
                | Rvalue::ToString(_)
                | Rvalue::Concat(_)
                | Rvalue::ConcatInt { .. }
                | Rvalue::StrLen(_)
                | Rvalue::StrByteSize(_)
                | Rvalue::CharAt(_, _, _)
                | Rvalue::ByteAt(_, _, _)
                | Rvalue::HashCode(_)
        ),
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
        Statement::Assign(
            _,
            Rvalue::New {
                ctor: Some(_),
                args,
                ..
            },
        ) => Some((vec![true; args.len()], args)),
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
