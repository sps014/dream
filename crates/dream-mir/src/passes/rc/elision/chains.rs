use super::super::RcKey;
use super::super::is_pure_rvalue;
use super::super::is_transparent_stmt;
use crate::BlockId;
use crate::MirFunction;
use crate::Place;
use crate::Statement;
use crate::Terminator;
use indexmap::IndexMap;
use indexmap::IndexSet;
use std::collections::BTreeMap;

pub(super) fn elide_goto_chains(
    func: &mut MirFunction,
    analyses: &mut crate::passes::FunctionAnalyses,
) -> bool {
    let preds = analyses.predecessors(func);
    let n = func.blocks.len();
    let mut visited = vec![false; n];
    let mut changed = false;
    for bi in 0..n {
        if visited[bi] {
            continue;
        }
        let start = BlockId(bi as u32);
        if is_goto_chain_continuation(func, &preds, start) {
            continue;
        }
        let chain = goto_chain(func, &preds, start);
        for &b in &chain {
            visited[b.0 as usize] = true;
        }
        if elide_region(func, &chain) {
            changed = true;
        }
    }
    changed
}

/// `Retain` in the If-block  in the unique join, with both arms transparent.
pub(super) fn chains_transparent(func: &MirFunction, chain: &[BlockId]) -> bool {
    chain.iter().all(|&b| block_stmts_transparent(func, b))
}

pub(super) fn block_stmts_transparent(func: &MirFunction, b: BlockId) -> bool {
    func.blocks[b.0 as usize]
        .stmts
        .iter()
        .all(is_transparent_stmt)
}

/// True when `b` is the unique successor of a unique predecessor that ends in `Goto(b)`.
pub(super) fn is_goto_chain_continuation(
    func: &MirFunction,
    preds: &[Vec<BlockId>],
    b: BlockId,
) -> bool {
    let p = &preds[b.0 as usize];
    if p.len() != 1 {
        return false;
    }
    let pred = p[0];
    matches!(
        func.blocks[pred.0 as usize].terminator,
        Terminator::Goto(t) if t == b
    )
}

/// Straight-line region starting at `start`: follow `Goto` edges while the successor has exactly
/// that block as its unique predecessor.
pub(super) fn goto_chain(
    func: &MirFunction,
    preds: &[Vec<BlockId>],
    start: BlockId,
) -> Vec<BlockId> {
    let mut chain = vec![start];
    let mut cur = start;
    loop {
        match func.blocks[cur.0 as usize].terminator {
            Terminator::Goto(next)
                if preds[next.0 as usize].len() == 1 && preds[next.0 as usize][0] == cur =>
            {
                chain.push(next);
                cur = next;
            }
            _ => break,
        }
    }
    chain
}

/// Maximal unique-pred Goto chain that *ends* at `end` (walking predecessors).
pub(super) fn goto_chain_ending_at(
    func: &MirFunction,
    preds: &[Vec<BlockId>],
    end: BlockId,
) -> Vec<BlockId> {
    let mut rev = vec![end];
    let mut cur = end;
    loop {
        let p = &preds[cur.0 as usize];
        if p.len() != 1 {
            break;
        }
        let pred = p[0];
        if !matches!(
            func.blocks[pred.0 as usize].terminator,
            Terminator::Goto(t) if t == cur
        ) {
            break;
        }
        rev.push(pred);
        cur = pred;
    }
    rev.reverse();
    rev
}

/// Runs the retain/release cancel sweep across the concatenated statements of `chain`.
pub(super) fn elide_region(func: &mut MirFunction, chain: &[BlockId]) -> bool {
    let mut locs: Vec<(usize, usize)> = Vec::new();
    for &b in chain {
        let bi = b.0 as usize;
        for si in 0..func.blocks[bi].stmts.len() {
            locs.push((bi, si));
        }
    }
    let n = locs.len();
    let mut keep = vec![true; n];
    let mut region_changed = false;
    let mut pending: IndexMap<RcKey, Vec<usize>> = IndexMap::new();
    for i in 0..n {
        let (bi, si) = locs[i];
        match &func.blocks[bi].stmts[si] {
            Statement::Retain(op) => {
                if let Some(key) = RcKey::of(op) {
                    pending.entry(key).or_default().push(i);
                }
            }
            Statement::Release(op) => {
                if let Some(key) = RcKey::of(op)
                    && let Some(stack) = pending.get_mut(&key)
                    && let Some(retain_idx) = stack.pop()
                {
                    keep[retain_idx] = false;
                    keep[i] = false;
                    region_changed = true;
                    continue;
                }
                // An unmatched (or differently-keyed) `Release` may drop the last count of
                // an object some *other* pending key aliases — not provably safe to ignore.
                pending.clear();
            }
            Statement::Assign(Place::Local(dst), rvalue) if is_pure_rvalue(rvalue) => {
                let key = RcKey::Local(*dst);
                pending.swap_remove(&key);
            }
            Statement::Print { .. }
            | Statement::DebugLine(_)
            | Statement::SourceLine(_)
            | Statement::Nop => {}
            _ => {
                pending.clear();
            }
        }
    }
    if !region_changed {
        return false;
    }
    let mut drop_at: BTreeMap<usize, IndexSet<usize>> = BTreeMap::new();
    for (i, &(bi, si)) in locs.iter().enumerate() {
        if !keep[i] {
            drop_at.entry(bi).or_default().insert(si);
        }
    }
    for (bi, drop_set) in drop_at {
        let mut idx = 0;
        func.blocks[bi].stmts.retain(|_| {
            let keep_stmt = !drop_set.contains(&idx);
            idx += 1;
            keep_stmt
        });
    }
    true
}
