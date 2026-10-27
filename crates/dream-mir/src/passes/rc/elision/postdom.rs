use super::super::RcKey;
use super::super::is_transparent_stmt;
use crate::BlockId;
use crate::MirFunction;
use crate::Statement;
use crate::passes::cfg;
use std::collections::BTreeSet;

/// Cancel `Retain`/`Release` when the release postdominates the retain, the retain dominates the
/// release, and every block on the SESE region between them is transparent (generalizes diamonds).
pub(super) fn elide_postdom_transparent(
    func: &mut MirFunction,
    analyses: &mut crate::passes::FunctionAnalyses,
) -> bool {
    elide_postdom_by(func, analyses, |func, key, dom, pdom, rb, rs, eb, es| {
        region_all(func, dom, pdom, (rb, rs, eb, es), |_, _, stmt| {
            is_transparent_stmt(stmt)
                && !matches!(stmt,
                Statement::Assign(crate::Place::Local(dest), _) if key == RcKey::Local(*dest))
        })
    })
}

pub(super) fn elide_postdom_by(
    func: &mut MirFunction,
    analyses: &mut crate::passes::FunctionAnalyses,
    safe: impl Fn(
        &MirFunction,
        RcKey,
        &cfg::DomTree,
        &cfg::PostDomTree,
        BlockId,
        usize,
        BlockId,
        usize,
    ) -> bool,
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
            if !region_runs_once(func, rb, eb) || !safe(func, rk, &dom, &pdom, rb, rs, eb, es) {
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
            if crate::verify::enabled() {
                assert!(
                    safe(func, rk, &dom, &pdom, rb, rs, eb, es),
                    "invalid CFG RC elision proof"
                );
            }
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

pub(super) fn region_all(
    func: &MirFunction,
    dom: &cfg::DomTree,
    pdom: &cfg::PostDomTree,
    (rb, rs, eb, es): (BlockId, usize, BlockId, usize),
    mut safe: impl FnMut(usize, usize, &Statement) -> bool,
) -> bool {
    let n = func.blocks.len();
    for bi in 0..n {
        let b = BlockId(bi as u32);
        if !dom.dominates(rb, b) || !pdom.postdominates(eb, b) {
            continue;
        }
        let stmts = &func.blocks[bi].stmts;
        if b == rb && b == eb {
            return stmts[rs + 1..es]
                .iter()
                .enumerate()
                .all(|(offset, stmt)| safe(bi, rs + 1 + offset, stmt));
        }
        if b == rb {
            if !stmts[rs + 1..]
                .iter()
                .enumerate()
                .all(|(offset, stmt)| safe(bi, rs + 1 + offset, stmt))
            {
                return false;
            }
            continue;
        }
        if b == eb {
            if !stmts[..es]
                .iter()
                .enumerate()
                .all(|(si, stmt)| safe(bi, si, stmt))
            {
                return false;
            }
            continue;
        }
        if !stmts
            .iter()
            .enumerate()
            .all(|(si, stmt)| safe(bi, si, stmt))
        {
            return false;
        }
    }
    true
}

/// The same retain cannot execute again before its proposed matching release.
fn region_runs_once(func: &MirFunction, retain: BlockId, release: BlockId) -> bool {
    if retain == release {
        return true;
    }
    let mut seen = BTreeSet::new();
    let mut pending = func.blocks[retain.0 as usize].terminator.successors();
    while let Some(block) = pending.pop() {
        if block == retain {
            return false;
        }
        if block == release || !seen.insert(block) {
            continue;
        }
        pending.extend(func.blocks[block.0 as usize].terminator.successors());
    }
    true
}
