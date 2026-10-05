use super::super::is_transparent_stmt;
use super::super::RcKey;
use crate::passes::cfg;
use crate::BlockId;
use crate::MirFunction;
use crate::Statement;
use std::collections::BTreeSet;

/// Cancel `Retain`/`Release` when the release postdominates the retain, the retain dominates the
/// release, and every block on the SESE region between them is transparent (generalizes diamonds).
pub(super) fn elide_postdom_transparent(
    func: &mut MirFunction,
    analyses: &mut crate::passes::FunctionAnalyses,
) -> bool {
    let dom = analyses.dominators(func);
    let pdom = analyses.postdominators(func);
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

pub(super) fn retain_release_ordered(rb: BlockId, rs: usize, eb: BlockId, es: usize) -> bool {
    if rb == eb {
        rs < es
    } else {
        true // dominance/postdominance constrain cross-block order
    }
}

pub(super) fn region_transparent(
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
