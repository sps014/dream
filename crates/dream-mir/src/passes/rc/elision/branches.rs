use super::chains::block_stmts_transparent;
use super::chains::chains_transparent;
use super::chains::elide_region;
use super::chains::goto_chain;
use super::chains::goto_chain_ending_at;
use crate::BlockId;
use crate::MirFunction;
use crate::Terminator;
use std::collections::BTreeSet;

pub(super) fn elide_transparent_diamonds(
    func: &mut MirFunction,
    analyses: &mut crate::passes::FunctionAnalyses,
) -> bool {
    let preds = analyses.predecessors(func);
    let n = func.blocks.len();
    let mut changed = false;
    for bi in 0..n {
        let head = BlockId(bi as u32);
        let Terminator::If {
            then_blk, else_blk, ..
        } = func.blocks[bi].terminator
        else {
            continue;
        };
        let then_chain = goto_chain(func, &preds, then_blk);
        let else_chain = goto_chain(func, &preds, else_blk);
        let Some(&then_end) = then_chain.last() else {
            continue;
        };
        let Some(&else_end) = else_chain.last() else {
            continue;
        };
        let then_join = match func.blocks[then_end.0 as usize].terminator {
            Terminator::Goto(j) => j,
            _ => continue,
        };
        let else_join = match func.blocks[else_end.0 as usize].terminator {
            Terminator::Goto(j) => j,
            _ => continue,
        };
        if then_join != else_join {
            continue;
        }
        let join = then_join;
        // Join's predecessors must be exactly the two arm ends (no other entrances).
        let join_preds = &preds[join.0 as usize];
        if join_preds.len() != 2 {
            continue;
        }
        let pred_set: BTreeSet<BlockId> = join_preds.iter().copied().collect();
        if !pred_set.contains(&then_end) || !pred_set.contains(&else_end) {
            continue;
        }
        if !chains_transparent(func, &then_chain) || !chains_transparent(func, &else_chain) {
            continue;
        }
        // Head + join form the elision region (arms are transparent and contribute no RC ops).
        if elide_region(func, &[head, join]) {
            changed = true;
        }
    }
    changed
}

/// `Retain` in the unique preheader  in the unique exit, with a transparent loop body.
pub(super) fn elide_around_transparent_loops(
    func: &mut MirFunction,
    analyses: &mut crate::passes::FunctionAnalyses,
) -> bool {
    let preds = analyses.predecessors(func);
    let loops = analyses.natural_loops(func);
    let mut changed = false;
    for lp in loops.iter() {
        if !lp.body.iter().all(|&b| block_stmts_transparent(func, b)) {
            continue;
        }
        let header = lp.header;
        let preheaders: Vec<BlockId> = preds[header.0 as usize]
            .iter()
            .copied()
            .filter(|p| !lp.body.contains(p))
            .collect();
        if preheaders.len() != 1 {
            continue;
        }
        let ph = preheaders[0];
        let mut exits = BTreeSet::new();
        for &b in &lp.body {
            for s in func.blocks[b.0 as usize].terminator.successors() {
                if !lp.body.contains(&s) {
                    exits.insert(s);
                }
            }
        }
        if exits.len() != 1 {
            continue;
        }
        let exit = *exits.iter().next().expect("len == 1");
        // Include the Goto-chain that ends at the preheader so a Retain earlier in that chain matches.
        let mut region = goto_chain_ending_at(func, &preds, ph);
        if !region.contains(&exit) {
            region.push(exit);
        }
        if elide_region(func, &region) {
            changed = true;
        }
    }
    changed
}
