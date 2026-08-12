//! Shared control-flow analyses for the optimization passes: predecessors, a dominator tree
//! (Cooper-Harvey-Kennedy), and natural-loop / back-edge detection. Kept pass-agnostic so LICM,
//! loop unrolling, and any future structural pass share one implementation.
//!
//! Determinism: every result is indexed by block position or held in ordered containers, and blocks
//! are always visited in their `Vec` order, so two runs produce identical output.

use crate::{BlockId, MirFunction, Terminator};
use std::collections::BTreeSet;

/// Predecessor lists indexed by block position. Only edges among the function's blocks are
/// recorded; a block with no predecessors (the entry, or an unreachable block) has an empty list.
pub(crate) fn predecessors(func: &MirFunction) -> Vec<Vec<BlockId>> {
    let mut preds = vec![Vec::new(); func.blocks.len()];
    for (i, block) in func.blocks.iter().enumerate() {
        for succ in block.terminator.successors() {
            preds[succ.0 as usize].push(BlockId(i as u32));
        }
    }
    preds
}

/// Reverse postorder of the blocks reachable from `entry` (entry first). Unreachable blocks are
/// omitted.
pub(crate) fn reverse_postorder(func: &MirFunction) -> Vec<BlockId> {
    let n = func.blocks.len();
    let mut visited = vec![false; n];
    let mut post = Vec::with_capacity(n);
    // Iterative postorder DFS: push (block, child-cursor) frames.
    let mut stack: Vec<(BlockId, usize)> = vec![(func.entry, 0)];
    visited[func.entry.0 as usize] = true;
    while let Some((b, cursor)) = stack.last().copied() {
        let succs = func.block(b).terminator.successors();
        if cursor < succs.len() {
            stack.last_mut().unwrap().1 += 1;
            let s = succs[cursor];
            if !visited[s.0 as usize] {
                visited[s.0 as usize] = true;
                stack.push((s, 0));
            }
        } else {
            post.push(b);
            stack.pop();
        }
    }
    post.reverse();
    post
}

/// The immediate dominator of every block, indexed by block position. `None` marks the entry
/// (which has no dominator) and any block unreachable from the entry.
pub(crate) struct DomTree {
    idom: Vec<Option<BlockId>>,
    rpo_index: Vec<u32>,
}

impl DomTree {
    /// Computes the dominator tree via the Cooper-Harvey-Kennedy iterative algorithm.
    pub(crate) fn new(func: &MirFunction) -> DomTree {
        let n = func.blocks.len();
        let preds = predecessors(func);
        let rpo = reverse_postorder(func);
        let mut rpo_index = vec![u32::MAX; n];
        for (i, b) in rpo.iter().enumerate() {
            rpo_index[b.0 as usize] = i as u32;
        }

        let mut idom: Vec<Option<BlockId>> = vec![None; n];
        idom[func.entry.0 as usize] = Some(func.entry);

        let mut changed = true;
        while changed {
            changed = false;
            for &b in &rpo {
                if b == func.entry {
                    continue;
                }
                let mut new_idom: Option<BlockId> = None;
                for &p in &preds[b.0 as usize] {
                    if idom[p.0 as usize].is_none() {
                        continue;
                    }
                    new_idom = Some(match new_idom {
                        None => p,
                        Some(cur) => intersect(&idom, &rpo_index, p, cur),
                    });
                }
                if idom[b.0 as usize] != new_idom {
                    idom[b.0 as usize] = new_idom;
                    changed = true;
                }
            }
        }

        DomTree { idom, rpo_index }
    }

    /// True if `a` dominates `b` (every path from entry to `b` passes through `a`). A block always
    /// dominates itself. Unreachable blocks dominate nothing but themselves.
    pub(crate) fn dominates(&self, a: BlockId, b: BlockId) -> bool {
        if self.rpo_index[b.0 as usize] == u32::MAX {
            return a == b;
        }
        let mut cur = b;
        loop {
            if cur == a {
                return true;
            }
            match self.idom[cur.0 as usize] {
                Some(next) if next != cur => cur = next,
                _ => return false,
            }
        }
    }
}

/// Immediate postdominator of every block. Built as dominators of the reverse CFG with a synthetic
/// exit that every function-exit terminator edges to. `a` postdominates `b` when every path from
/// `b` to a function exit passes through `a`.
pub(crate) struct PostDomTree {
    /// Immediate postdominator per block; `None` for the synthetic exit and unreachable-from-exit
    /// blocks. Real blocks that only exit through `Return`/`Unreachable`/… postdominate themselves
    /// via the chain ending at the synthetic exit.
    ipdom: Vec<Option<BlockId>>,
    /// Reverse-postorder index on the reverse CFG (exit-first). `u32::MAX` = unreachable from exit.
    rpo_index: Vec<u32>,
    /// Synthetic exit block id (`blocks.len()`), not a real MIR block.
    exit: BlockId,
}

impl PostDomTree {
    pub(crate) fn new(func: &MirFunction) -> PostDomTree {
        let n = func.blocks.len();
        let exit = BlockId(n as u32);
        // Reverse CFG: forward edge `a → b` becomes `b → a`. Dominators of that graph (rooted at a
        // synthetic exit) are postdominators of the original. Keep both successor and predecessor
        // lists — DFS walks successors; Cooper–Harvey–Kennedy needs predecessors.
        let mut rev_succs: Vec<Vec<BlockId>> = vec![Vec::new(); n + 1];
        let mut rev_preds: Vec<Vec<BlockId>> = vec![Vec::new(); n + 1];
        for (i, block) in func.blocks.iter().enumerate() {
            let b = BlockId(i as u32);
            let succs = block.terminator.successors();
            if succs.is_empty() || is_exit_terminator(&block.terminator) {
                // b → exit (forward) ⇒ exit → b (reverse)
                rev_succs[exit.0 as usize].push(b);
                rev_preds[b.0 as usize].push(exit);
            }
            for s in succs {
                // b → s (forward) ⇒ s → b (reverse)
                rev_succs[s.0 as usize].push(b);
                rev_preds[b.0 as usize].push(s);
            }
        }

        let mut visited = vec![false; n + 1];
        let mut post = Vec::with_capacity(n + 1);
        let mut stack: Vec<(BlockId, usize)> = vec![(exit, 0)];
        visited[exit.0 as usize] = true;
        while let Some((b, cursor)) = stack.last().copied() {
            let succs = &rev_succs[b.0 as usize];
            if cursor < succs.len() {
                stack.last_mut().unwrap().1 += 1;
                let s = succs[cursor];
                if !visited[s.0 as usize] {
                    visited[s.0 as usize] = true;
                    stack.push((s, 0));
                }
            } else {
                post.push(b);
                stack.pop();
            }
        }
        post.reverse();
        let mut rpo_index = vec![u32::MAX; n + 1];
        for (i, b) in post.iter().enumerate() {
            rpo_index[b.0 as usize] = i as u32;
        }

        let mut idom: Vec<Option<BlockId>> = vec![None; n + 1];
        idom[exit.0 as usize] = Some(exit);
        let mut changed = true;
        while changed {
            changed = false;
            for &b in &post {
                if b == exit {
                    continue;
                }
                let mut new_idom: Option<BlockId> = None;
                for &p in &rev_preds[b.0 as usize] {
                    if idom[p.0 as usize].is_none() {
                        continue;
                    }
                    new_idom = Some(match new_idom {
                        None => p,
                        Some(cur) => intersect(&idom, &rpo_index, p, cur),
                    });
                }
                if idom[b.0 as usize] != new_idom {
                    idom[b.0 as usize] = new_idom;
                    changed = true;
                }
            }
        }

        PostDomTree {
            ipdom: idom,
            rpo_index,
            exit,
        }
    }

    /// True if `a` postdominates `b` (every path from `b` to a function exit passes through `a`).
    pub(crate) fn postdominates(&self, a: BlockId, b: BlockId) -> bool {
        if a == b {
            return true;
        }
        if (b.0 as usize) >= self.rpo_index.len() || self.rpo_index[b.0 as usize] == u32::MAX {
            return false;
        }
        let mut cur = b;
        loop {
            match self.ipdom[cur.0 as usize] {
                Some(next) if next != cur => {
                    if next == a {
                        return true;
                    }
                    if next == self.exit {
                        return false;
                    }
                    cur = next;
                }
                _ => return false,
            }
        }
    }
}

fn is_exit_terminator(t: &Terminator) -> bool {
    matches!(
        t,
        Terminator::Return(_)
            | Terminator::Unreachable
            | Terminator::AsyncComplete(_)
            | Terminator::TailCall { .. }
    )
}

/// Walks two dominator-tree fingers toward the entry until they meet (CHK `intersect`).
fn intersect(
    idom: &[Option<BlockId>],
    rpo_index: &[u32],
    mut a: BlockId,
    mut b: BlockId,
) -> BlockId {
    while a != b {
        while rpo_index[a.0 as usize] > rpo_index[b.0 as usize] {
            a = idom[a.0 as usize].expect("finger has an idom");
        }
        while rpo_index[b.0 as usize] > rpo_index[a.0 as usize] {
            b = idom[b.0 as usize].expect("finger has an idom");
        }
    }
    a
}

/// A natural loop: its header, the set of blocks in its body (header included), and the latch
/// blocks whose terminators branch back to the header.
pub(crate) struct NaturalLoop {
    pub header: BlockId,
    pub body: BTreeSet<BlockId>,
    pub latches: Vec<BlockId>,
}

/// Finds every natural loop, one per header (loops sharing a header are merged). Result is ordered
/// by header for determinism.
pub(crate) fn natural_loops(func: &MirFunction) -> Vec<NaturalLoop> {
    let dom = DomTree::new(func);
    let preds = predecessors(func);

    // Group back edges by header: an edge `tail -> header` is a back edge iff `header` dominates
    // `tail`.
    let mut loops: Vec<NaturalLoop> = Vec::new();
    for (i, block) in func.blocks.iter().enumerate() {
        let tail = BlockId(i as u32);
        for header in block.terminator.successors() {
            if !dom.dominates(header, tail) {
                continue;
            }
            let body = loop_body(&preds, header, tail);
            match loops.iter_mut().find(|l| l.header == header) {
                Some(existing) => {
                    existing.body.extend(body);
                    existing.latches.push(tail);
                }
                None => loops.push(NaturalLoop {
                    header,
                    body,
                    latches: vec![tail],
                }),
            }
        }
    }
    loops.sort_by_key(|l| l.header.0);
    loops
}

/// The set of blocks in the natural loop of the back edge `tail -> header`: the header plus every
/// block that reaches `tail` without passing through the header.
fn loop_body(preds: &[Vec<BlockId>], header: BlockId, tail: BlockId) -> BTreeSet<BlockId> {
    let mut body = BTreeSet::new();
    body.insert(header);
    if tail != header {
        body.insert(tail);
        let mut stack = vec![tail];
        while let Some(x) = stack.pop() {
            for &p in &preds[x.0 as usize] {
                if body.insert(p) {
                    stack.push(p);
                }
            }
        }
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use crate::{Const, Operand, Terminator};
    use dream_types::TypeInterner;

    /// Builds `entry -> cond -> (body -> cond | after)` — a single counted loop with header `cond`.
    fn loop_fn() -> MirFunction {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.int());
        let cond = b.new_block();
        let body = b.new_block();
        let after = b.new_block();
        b.terminate(Terminator::Goto(cond));
        b.switch_to(cond);
        b.terminate(Terminator::If {
            cond: Operand::Const(Const::Bool(true)),
            then_blk: body,
            else_blk: after,
        });
        b.switch_to(body);
        b.terminate(Terminator::Goto(cond)); // back edge
        b.switch_to(after);
        b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(0)))));
        b.finish()
    }

    #[test]
    fn detects_single_natural_loop() {
        let func = loop_fn();
        let loops = natural_loops(&func);
        assert_eq!(loops.len(), 1, "one loop expected");
        let l = &loops[0];
        assert_eq!(l.header, BlockId(1), "header is the cond block");
        assert!(l.body.contains(&BlockId(1)) && l.body.contains(&BlockId(2)));
        assert!(
            !l.body.contains(&BlockId(3)),
            "after-block is outside the loop"
        );
        assert_eq!(l.latches, vec![BlockId(2)]);
    }

    #[test]
    fn dominator_relationships() {
        let func = loop_fn();
        let dom = DomTree::new(&func);
        assert!(dom.dominates(BlockId(0), BlockId(3)), "entry dominates all");
        assert!(
            dom.dominates(BlockId(1), BlockId(2)),
            "header dominates body"
        );
        assert!(
            !dom.dominates(BlockId(2), BlockId(3)),
            "body does not dominate after"
        );
    }
}
