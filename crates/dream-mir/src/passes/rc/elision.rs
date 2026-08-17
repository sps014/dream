//! [`RcElision`]: cancel redundant Retain/Release pairs.

use super::{is_pure_rvalue, is_transparent_stmt, RcKey};
use crate::passes::cfg;
use crate::passes::MirPass;
use crate::{BlockId, MirFunction, Place, Statement, Terminator};
use dream_types::TypeInterner;
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub struct RcElision;

impl MirPass for RcElision {
    fn name(&self) -> &'static str {
        "rc-elision"
    }

    /// Cancels a `Retain(x)`/`Release(x)` pair on the same identity `x` when separated only by
    /// provably side-effect-free statements along:
    /// - unique-pred `Goto` chains (straight-line CFG),
    /// - transparent diamonds (both arms barrier-free, unique join),
    /// - transparent natural loops (body has no RC barriers; pair sandwiches the loop).
    ///
    /// The refcount an object carries at any point is observable (`Debug.ref_count`, `del()`). So
    /// **any** statement that could call into other code, allocate, or itself retain/release a
    /// (possibly aliased) object is a hard barrier. Only a small whitelist may pass through.
    /// Rule: never under-retain.
    fn run(&self, func: &mut MirFunction, _interner: &TypeInterner) -> bool {
        let mut changed = false;
        // Fixpoint: diamond/loop/postdom elision can expose new Goto-chain pairs and vice versa.
        for _ in 0..8 {
            let mut round = false;
            round |= elide_goto_chains(func);
            round |= elide_transparent_diamonds(func);
            round |= elide_around_transparent_loops(func);
            round |= elide_postdom_transparent(func);
            if !round {
                break;
            }
            changed = true;
        }
        changed
    }
}

fn elide_goto_chains(func: &mut MirFunction) -> bool {
    let preds = cfg::predecessors(func);
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

/// `Retain` in the If-block / `Release` in the unique join, with both arms transparent.
fn elide_transparent_diamonds(func: &mut MirFunction) -> bool {
    let preds = cfg::predecessors(func);
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

/// `Retain` in the unique preheader / `Release` in the unique exit, with a transparent loop body.
fn elide_around_transparent_loops(func: &mut MirFunction) -> bool {
    let preds = cfg::predecessors(func);
    let loops = cfg::natural_loops(func);
    let mut changed = false;
    for lp in loops {
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

/// Cancel `Retain`/`Release` when the release postdominates the retain, the retain dominates the
/// release, and every block on the SESE region between them is transparent (generalizes diamonds).
fn elide_postdom_transparent(func: &mut MirFunction) -> bool {
    let dom = cfg::DomTree::new(func);
    let pdom = cfg::PostDomTree::new(func);
    let n = func.blocks.len();

    // Collect retain/release sites as (block, stmt_idx, key).
    let mut retains: Vec<(BlockId, usize, RcKey)> = Vec::new();
    let mut releases: Vec<(BlockId, usize, RcKey)> = Vec::new();
    for bi in 0..n {
        let b = BlockId(bi as u32);
        for (si, stmt) in func.blocks[bi].stmts.iter().enumerate() {
            match stmt {
                Statement::Retain(op) => {
                    if let Some(k) = RcKey::of(op) {
                        retains.push((b, si, k));
                    }
                }
                Statement::Release(op) => {
                    if let Some(k) = RcKey::of(op) {
                        releases.push((b, si, k));
                    }
                }
                _ => {}
            }
        }
    }

    // Prefer nearest release after each retain (deterministic: block order, then stmt order).
    let mut drop_retain: BTreeSet<(BlockId, usize)> = BTreeSet::new();
    let mut drop_release: BTreeSet<(BlockId, usize)> = BTreeSet::new();
    for &(rb, rs, rk) in &retains {
        if drop_retain.contains(&(rb, rs)) {
            continue;
        }
        let mut best: Option<(BlockId, usize)> = None;
        for &(eb, es, ek) in &releases {
            if ek != rk || drop_release.contains(&(eb, es)) {
                continue;
            }
            if !retain_release_ordered(rb, rs, eb, es) {
                continue;
            }
            if !dom.dominates(rb, eb) || !pdom.postdominates(eb, rb) {
                continue;
            }
            if !region_transparent(func, &dom, &pdom, rb, rs, eb, es) {
                continue;
            }
            match best {
                None => best = Some((eb, es)),
                Some((bb, bs)) => {
                    if (eb, es) < (bb, bs) {
                        best = Some((eb, es));
                    }
                }
            }
        }
        if let Some((eb, es)) = best {
            drop_retain.insert((rb, rs));
            drop_release.insert((eb, es));
        }
    }

    if drop_retain.is_empty() {
        return false;
    }
    for bi in 0..n {
        let b = BlockId(bi as u32);
        let mut idx = 0;
        func.blocks[bi].stmts.retain(|_| {
            let keep = !drop_retain.contains(&(b, idx)) && !drop_release.contains(&(b, idx));
            idx += 1;
            keep
        });
    }
    true
}

fn retain_release_ordered(rb: BlockId, rs: usize, eb: BlockId, es: usize) -> bool {
    if rb == eb {
        rs < es
    } else {
        true // dominance/postdominance constrain cross-block order
    }
}

fn region_transparent(
    func: &MirFunction,
    dom: &cfg::DomTree,
    pdom: &cfg::PostDomTree,
    rb: BlockId,
    rs: usize,
    eb: BlockId,
    es: usize,
) -> bool {
    let n = func.blocks.len();
    for bi in 0..n {
        let b = BlockId(bi as u32);
        if !dom.dominates(rb, b) || !pdom.postdominates(eb, b) {
            continue;
        }
        let stmts = &func.blocks[bi].stmts;
        if b == rb && b == eb {
            return stmts[rs + 1..es].iter().all(is_transparent_stmt);
        }
        if b == rb {
            if !stmts[rs + 1..].iter().all(is_transparent_stmt) {
                return false;
            }
            continue;
        }
        if b == eb {
            if !stmts[..es].iter().all(is_transparent_stmt) {
                return false;
            }
            continue;
        }
        if !stmts.iter().all(is_transparent_stmt) {
            return false;
        }
    }
    true
}

fn chains_transparent(func: &MirFunction, chain: &[BlockId]) -> bool {
    chain.iter().all(|&b| block_stmts_transparent(func, b))
}

fn block_stmts_transparent(func: &MirFunction, b: BlockId) -> bool {
    func.blocks[b.0 as usize]
        .stmts
        .iter()
        .all(is_transparent_stmt)
}

/// True when `b` is the unique successor of a unique predecessor that ends in `Goto(b)`.
fn is_goto_chain_continuation(func: &MirFunction, preds: &[Vec<BlockId>], b: BlockId) -> bool {
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
fn goto_chain(func: &MirFunction, preds: &[Vec<BlockId>], start: BlockId) -> Vec<BlockId> {
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
fn goto_chain_ending_at(func: &MirFunction, preds: &[Vec<BlockId>], end: BlockId) -> Vec<BlockId> {
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
fn elide_region(func: &mut MirFunction, chain: &[BlockId]) -> bool {
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
    let mut pending: std::collections::HashMap<RcKey, Vec<usize>> =
        std::collections::HashMap::new();
    for i in 0..n {
        let (bi, si) = locs[i];
        match &func.blocks[bi].stmts[si] {
            Statement::Retain(op) => {
                if let Some(key) = RcKey::of(op) {
                    pending.entry(key).or_default().push(i);
                }
            }
            Statement::Release(op) => {
                if let Some(key) = RcKey::of(op) {
                    if let Some(stack) = pending.get_mut(&key) {
                        if let Some(retain_idx) = stack.pop() {
                            keep[retain_idx] = false;
                            keep[i] = false;
                            region_changed = true;
                            continue;
                        }
                    }
                }
                // An unmatched (or differently-keyed) `Release` may drop the last count of
                // an object some *other* pending key aliases — not provably safe to ignore.
                pending.clear();
            }
            Statement::Assign(Place::Local(dst), rvalue) if is_pure_rvalue(rvalue) => {
                let key = RcKey::Local(*dst);
                pending.remove(&key);
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
    let mut drop_at: BTreeMap<usize, HashSet<usize>> = BTreeMap::new();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use crate::passes::rc::RcInsertion;
    use crate::{Local, Operand, Place, Rvalue, Terminator};

    #[test]
    fn elides_adjacent_retain_release() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.void());
        b.push(Statement::Retain(Operand::Copy(Place::Local(Local(0)))));
        b.push(Statement::Release(Operand::Copy(Place::Local(Local(0)))));
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcElision.run(&mut func, &i));
        assert!(func.blocks[0].stmts.is_empty());
    }

    #[test]
    fn elides_retain_release_separated_by_pure_arithmetic() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.void());
        let x = b.new_local(i.string(), Some("x".into()));
        let tmp = b.new_local(i.int(), Some("tmp".into()));
        b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
        b.push(Statement::Assign(
            Place::Local(tmp),
            Rvalue::Binary(
                crate::BinOp::Add,
                Operand::Const(crate::Const::Int(1)),
                Operand::Const(crate::Const::Int(2)),
            ),
        ));
        b.push(Statement::Release(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcElision.run(&mut func, &i));
        assert_eq!(func.blocks[0].stmts.len(), 1);
        assert!(matches!(func.blocks[0].stmts[0], Statement::Assign(..)));
    }

    #[test]
    fn does_not_elide_across_a_call() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.void());
        let x = b.new_local(i.string(), Some("x".into()));
        let callee = crate::Callee {
            def: dream_types::DefId(0),
            args: vec![],
            ret: i.void(),
            take_params: vec![],
        };
        b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
        b.push(Statement::Call {
            callee,
            args: vec![],
        });
        b.push(Statement::Release(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(!RcElision.run(&mut func, &i));
        assert_eq!(func.blocks[0].stmts.len(), 3);
    }

    #[test]
    fn elides_across_a_pure_copy_to_a_different_local() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.void());
        let x = b.new_local(i.string(), Some("x".into()));
        let y = b.new_local(i.int(), Some("y".into()));
        b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
        b.push(Statement::Assign(
            Place::Local(y),
            Rvalue::Use(Operand::Copy(Place::Local(x))),
        ));
        b.push(Statement::Release(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcElision.run(&mut func, &i));
        assert_eq!(func.blocks[0].stmts.len(), 1);
    }

    #[test]
    fn nested_retains_cancel_innermost_first() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.void());
        let x = Local(0);
        b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
        b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
        b.push(Statement::Release(Operand::Copy(Place::Local(x))));
        b.push(Statement::Release(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcElision.run(&mut func, &i));
        assert!(func.blocks[0].stmts.is_empty());
    }

    #[test]
    fn elides_across_goto_chain() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.void());
        let x = b.new_local(i.string(), Some("x".into()));
        let mid = b.new_block();
        let end = b.new_block();
        b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::Goto(mid));
        b.switch_to(mid);
        b.terminate(Terminator::Goto(end));
        b.switch_to(end);
        b.push(Statement::Release(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcElision.run(&mut func, &i));
        assert!(func.blocks.iter().all(|bb| bb.stmts.is_empty()));
    }

    #[test]
    fn elides_across_transparent_diamond() {
        // Retain(x); if c { pure } else { pure }; Release(x)
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.void());
        let x = b.new_local(i.string(), Some("x".into()));
        let tmp = b.new_local(i.int(), Some("tmp".into()));
        let then_blk = b.new_block();
        let else_blk = b.new_block();
        let join = b.new_block();
        b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::If {
            cond: Operand::Const(crate::Const::Bool(true)),
            then_blk,
            else_blk,
        });
        b.switch_to(then_blk);
        b.push(Statement::Assign(
            Place::Local(tmp),
            Rvalue::Use(Operand::Const(crate::Const::Int(1))),
        ));
        b.terminate(Terminator::Goto(join));
        b.switch_to(else_blk);
        b.push(Statement::Assign(
            Place::Local(tmp),
            Rvalue::Use(Operand::Const(crate::Const::Int(2))),
        ));
        b.terminate(Terminator::Goto(join));
        b.switch_to(join);
        b.push(Statement::Release(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcElision.run(&mut func, &i));
        let retains = func
            .blocks
            .iter()
            .flat_map(|bb| &bb.stmts)
            .filter(|s| matches!(s, Statement::Retain(_)))
            .count();
        let releases = func
            .blocks
            .iter()
            .flat_map(|bb| &bb.stmts)
            .filter(|s| matches!(s, Statement::Release(_)))
            .count();
        assert_eq!(retains, 0);
        assert_eq!(releases, 0);
    }

    #[test]
    fn does_not_elide_diamond_with_call_in_arm() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.void());
        let x = b.new_local(i.string(), Some("x".into()));
        let then_blk = b.new_block();
        let else_blk = b.new_block();
        let join = b.new_block();
        let callee = crate::Callee {
            def: dream_types::DefId(0),
            args: vec![],
            ret: i.void(),
            take_params: vec![],
        };
        b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::If {
            cond: Operand::Const(crate::Const::Bool(true)),
            then_blk,
            else_blk,
        });
        b.switch_to(then_blk);
        b.push(Statement::Call {
            callee,
            args: vec![],
        });
        b.terminate(Terminator::Goto(join));
        b.switch_to(else_blk);
        b.terminate(Terminator::Goto(join));
        b.switch_to(join);
        b.push(Statement::Release(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(!RcElision.run(&mut func, &i));
    }

    #[test]
    fn elides_around_transparent_loop() {
        // entry: Retain(x); goto header
        // header: if c -> body else exit
        // body: pure; goto header
        // exit: Release(x)
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.void());
        let x = b.new_local(i.string(), Some("x".into()));
        let tmp = b.new_local(i.int(), Some("tmp".into()));
        let header = b.new_block();
        let body = b.new_block();
        let exit = b.new_block();
        b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::Goto(header));
        b.switch_to(header);
        b.terminate(Terminator::If {
            cond: Operand::Const(crate::Const::Bool(true)),
            then_blk: body,
            else_blk: exit,
        });
        b.switch_to(body);
        b.push(Statement::Assign(
            Place::Local(tmp),
            Rvalue::Binary(
                crate::BinOp::Add,
                Operand::Const(crate::Const::Int(1)),
                Operand::Const(crate::Const::Int(1)),
            ),
        ));
        b.terminate(Terminator::Goto(header));
        b.switch_to(exit);
        b.push(Statement::Release(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcElision.run(&mut func, &i));
        let retains = func
            .blocks
            .iter()
            .flat_map(|bb| &bb.stmts)
            .filter(|s| matches!(s, Statement::Retain(_)))
            .count();
        let releases = func
            .blocks
            .iter()
            .flat_map(|bb| &bb.stmts)
            .filter(|s| matches!(s, Statement::Release(_)))
            .count();
        assert_eq!(retains, 0);
        assert_eq!(releases, 0);
    }

    #[test]
    fn elides_across_transparent_switch_via_postdom() {
        // Retain; switch to three transparent arms; join Release — not a simple If diamond.
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.void());
        let x = b.new_local(i.string(), Some("x".into()));
        let a = b.new_block();
        let c = b.new_block();
        let d = b.new_block();
        let join = b.new_block();
        b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::Switch {
            value: Operand::Const(crate::Const::Int(0)),
            targets: vec![(0, a), (1, c)],
            default: d,
        });
        b.switch_to(a);
        b.terminate(Terminator::Goto(join));
        b.switch_to(c);
        b.terminate(Terminator::Goto(join));
        b.switch_to(d);
        b.terminate(Terminator::Goto(join));
        b.switch_to(join);
        b.push(Statement::Release(Operand::Copy(Place::Local(x))));
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcElision.run(&mut func, &i));
        let retains = func
            .blocks
            .iter()
            .flat_map(|bb| &bb.stmts)
            .filter(|s| matches!(s, Statement::Retain(_)))
            .count();
        assert_eq!(retains, 0);
    }

    #[test]
    fn last_use_move_at_forward_join() {
        // s = "x"; if c { } else { }; t = s; return t; — move at the join after a diamond.
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.string());
        let s = b.new_local(i.string(), Some("s".into()));
        let t = b.new_local(i.string(), Some("t".into()));
        let then_blk = b.new_block();
        let else_blk = b.new_block();
        let join = b.new_block();
        b.assign(
            Place::Local(s),
            Rvalue::Use(Operand::Const(crate::Const::Str("x".into()))),
        );
        b.terminate(Terminator::If {
            cond: Operand::Const(crate::Const::Bool(true)),
            then_blk,
            else_blk,
        });
        b.switch_to(then_blk);
        b.terminate(Terminator::Goto(join));
        b.switch_to(else_blk);
        b.terminate(Terminator::Goto(join));
        b.switch_to(join);
        b.assign(Place::Local(t), Rvalue::Use(Operand::Copy(Place::Local(s))));
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &i));
        let nulls = func
            .blocks
            .iter()
            .flat_map(|bb| &bb.stmts)
            .filter(|s| {
                matches!(
                    s,
                    Statement::Assign(_, Rvalue::Use(Operand::Const(crate::Const::Null)))
                )
            })
            .count();
        assert_eq!(nulls, 1, "expected move of s into t at the join");
    }

    #[test]
    fn inserts_retain_on_borrowed_copy() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.string());
        let s = b.new_param(i.string(), Some("s".into()));
        let t = b.new_local(i.string(), Some("t".into()));
        b.assign(Place::Local(t), Rvalue::Use(Operand::Copy(Place::Local(s))));
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &i));
        let retains = func.blocks[0]
            .stmts
            .iter()
            .filter(|s| matches!(s, Statement::Retain(_)))
            .count();
        assert!(
            retains >= 1,
            "returning a copy of a borrowed param must retain"
        );
    }

    #[test]
    fn inserts_retain_on_borrowed_js_copy() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.js());
        let s = b.new_param(i.js(), Some("s".into()));
        let t = b.new_local(i.js(), Some("t".into()));
        b.assign(Place::Local(t), Rvalue::Use(Operand::Copy(Place::Local(s))));
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &i));
        let retains = func.blocks[0]
            .stmts
            .iter()
            .filter(|s| matches!(s, Statement::Retain(_)))
            .count();
        assert!(
            retains >= 1,
            "returning a copy of a borrowed js param must retain"
        );
    }

    #[test]
    fn returned_owned_local_is_not_released() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.string());
        let s = b.new_local(i.string(), Some("s".into()));
        b.assign(
            Place::Local(s),
            Rvalue::Use(Operand::Const(crate::Const::Str("x".into()))),
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(s)))));
        let mut func = b.finish();
        RcInsertion.run(&mut func, &i);
        let releases = func.blocks[0]
            .stmts
            .iter()
            .filter(|s| matches!(s, Statement::Release(_)))
            .count();
        assert_eq!(releases, 1);
        assert!(matches!(
            func.blocks[0].terminator,
            Terminator::Return(Some(Operand::Copy(Place::Local(l)))) if l == s
        ));
    }

    #[test]
    fn last_use_move_skips_retain_and_nulls_source() {
        // s = "x"; t = s; return t;  — after the copy, s is dead (t is returned), so move.
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.string());
        let s = b.new_local(i.string(), Some("s".into()));
        let t = b.new_local(i.string(), Some("t".into()));
        b.assign(
            Place::Local(s),
            Rvalue::Use(Operand::Const(crate::Const::Str("x".into()))),
        );
        b.assign(Place::Local(t), Rvalue::Use(Operand::Copy(Place::Local(s))));
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
        let mut func = b.finish();
        assert!(RcInsertion.run(&mut func, &i));
        // Expect: release s; assign s="x"; retain s; release t; assign t=s; assign s=null; (return t, no release t)
        // String literal binding retains; move of s→t skips retain on t and nulls s.
        let kinds: Vec<&str> = func.blocks[0]
            .stmts
            .iter()
            .map(|s| match s {
                Statement::Release(_) => "release",
                Statement::Assign(
                    Place::Local(_),
                    Rvalue::Use(Operand::Const(crate::Const::Null)),
                ) => "null",
                Statement::Assign(..) => "assign",
                Statement::Retain(_) => "retain",
                _ => "other",
            })
            .collect();
        assert!(
            kinds.contains(&"null"),
            "expected null of moved source, got {:?}",
            kinds
        );
        // No retain of t after the t=s assign (the move). There is still retain of s for the literal.
        let retain_count = kinds.iter().filter(|k| **k == "retain").count();
        assert_eq!(
            retain_count, 1,
            "only the string-literal retain should remain: {:?}",
            kinds
        );
    }

    #[test]
    fn no_move_when_source_still_live() {
        // s = "x"; t = s; return s; — s is live after the copy (returned), so cannot move.
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.string());
        let s = b.new_local(i.string(), Some("s".into()));
        let t = b.new_local(i.string(), Some("t".into()));
        b.assign(
            Place::Local(s),
            Rvalue::Use(Operand::Const(crate::Const::Str("x".into()))),
        );
        b.assign(Place::Local(t), Rvalue::Use(Operand::Copy(Place::Local(s))));
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(s)))));
        let mut func = b.finish();
        RcInsertion.run(&mut func, &i);
        let nulls = func.blocks[0]
            .stmts
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    Statement::Assign(_, Rvalue::Use(Operand::Const(crate::Const::Null)))
                )
            })
            .count();
        assert_eq!(nulls, 0, "source still live — no move");
        // `t` is an unused forwarding alias of `s`; it is a cursor and must not retain.
        let retains = func.blocks[0]
            .stmts
            .iter()
            .filter(|s| matches!(s, Statement::Retain(_)))
            .count();
        assert_eq!(retains, 1, "only the string-literal retain: {:?}", retains);
    }

    #[test]
    fn strings_not_early_released_after_print() {
        // Group/concat strings may alias a parent buffer; last-use destroy skips `string`.
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.void());
        let s = b.new_local(i.string(), Some("s".into()));
        let tmp = b.new_local(i.int(), Some("tmp".into()));
        b.assign(
            Place::Local(s),
            Rvalue::Use(Operand::Const(crate::Const::Str("x".into()))),
        );
        b.push(Statement::Print {
            arg: Operand::Copy(Place::Local(s)),
            ty: i.string(),
            newline: true,
        });
        b.push(Statement::SourceLine(2));
        b.assign(
            Place::Local(tmp),
            Rvalue::Binary(
                crate::BinOp::Add,
                Operand::Const(crate::Const::Int(1)),
                Operand::Const(crate::Const::Int(2)),
            ),
        );
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        RcInsertion.run(&mut func, &i);
        let stmts = &func.blocks[0].stmts;
        let print_at = stmts
            .iter()
            .position(|s| matches!(s, Statement::Print { .. }))
            .unwrap();
        let add_at = stmts
            .iter()
            .position(|s| matches!(s, Statement::Assign(_, Rvalue::Binary(..))))
            .unwrap();
        let early_release = stmts.iter().enumerate().any(|(idx, st)| {
            idx > print_at
                && idx < add_at
                && matches!(st, Statement::Release(Operand::Copy(Place::Local(l))) if *l == s)
        });
        assert!(
            !early_release,
            "string locals stay until scope exit, got {:?}",
            stmts
        );
    }

    #[test]
    fn early_release_after_last_js_use() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.void());
        let h = b.new_local(i.js(), Some("h".into()));
        let tmp = b.new_local(i.int(), Some("tmp".into()));
        b.assign(
            Place::Local(h),
            Rvalue::Use(Operand::Const(crate::Const::Null)),
        );
        b.push(Statement::Print {
            arg: Operand::Copy(Place::Local(h)),
            ty: i.js(),
            newline: true,
        });
        b.push(Statement::SourceLine(2));
        b.assign(
            Place::Local(tmp),
            Rvalue::Binary(
                crate::BinOp::Add,
                Operand::Const(crate::Const::Int(1)),
                Operand::Const(crate::Const::Int(2)),
            ),
        );
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        RcInsertion.run(&mut func, &i);
        let stmts = &func.blocks[0].stmts;
        let print_at = stmts
            .iter()
            .position(|s| matches!(s, Statement::Print { .. }))
            .unwrap();
        let add_at = stmts
            .iter()
            .position(|s| matches!(s, Statement::Assign(_, Rvalue::Binary(..))))
            .unwrap();
        let early = stmts.iter().enumerate().any(|(idx, st)| {
            idx > print_at
                && idx < add_at
                && matches!(st, Statement::Release(Operand::Copy(Place::Local(l))) if *l == h)
        });
        assert!(
            early,
            "expected Release of js handle between print and add, got {:?}",
            stmts
        );
    }

    #[test]
    fn no_early_release_of_loop_carried_local() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.void());
        let s = b.new_local(i.string(), Some("s".into()));
        let c = b.new_param(i.bool(), Some("c".into()));
        b.assign(
            Place::Local(s),
            Rvalue::Use(Operand::Const(crate::Const::Str("x".into()))),
        );
        let header = b.new_block();
        let body = b.new_block();
        let exit = b.new_block();
        b.terminate(Terminator::Goto(header));
        b.switch_to(header);
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(c)),
            then_blk: body,
            else_blk: exit,
        });
        b.switch_to(body);
        b.push(Statement::Print {
            arg: Operand::Copy(Place::Local(s)),
            ty: i.string(),
            newline: true,
        });
        b.terminate(Terminator::Goto(header));
        b.switch_to(exit);
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        RcInsertion.run(&mut func, &i);
        let body_stmts = &func.blocks[body.0 as usize].stmts;
        let body_early = body_stmts
            .iter()
            .any(|st| matches!(st, Statement::Release(Operand::Copy(Place::Local(l))) if *l == s));
        assert!(
            !body_early,
            "loop-carried s must not be released in the body: {:?}",
            body_stmts
        );
    }
}
