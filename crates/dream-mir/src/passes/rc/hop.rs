//! Chain-hop RC slimming: for the traversal idiom
//!
//! ```text
//! n = <union-field load of c>   (bind an alias of the holder)
//! Retain(n)                     — keeps the aliased object alive
//! ... reads of n ...
//! Release(c)                    — drops the original holder's count
//! c = <extract through n>; Retain(c)
//! Release(n)                    — releases what Retain(n) added
//! ```
//!
//! two independent savings are sound:
//!
//! 1. **Sink `Release(c)`** past statements that only *read* `n` (pure loads). The freed
//!    object must stay alive until `n`'s last read; everything between is pure except those
//!    reads. Sinking stops at the first write, call, or rebind of `c`.
//! 2. **Cancel `Retain(n)` / `Release(n)`** once the sink placed `Release(c)` after `n`'s
//!    last read: the extra count `Retain(n)` added exists only to protect reads that now all
//!    happen before `c`'s count drop. Deleting both leaves the object's survival to `c`'s own
//!    count through every read. If nothing else held the object, it is freed by the sunk
//!    `Release(c)` — after its last use.
//!
//! Per chain hop this turns 3 RMWs + 2 calls into 2 RMWs and no steady-state calls.

use super::super::MirPass;
use crate::{BlockId, Local, LocalDecl, MirFunction, Operand, Place, Rvalue, Statement};
use dream_types::TypeInterner;

pub struct HopElision;

impl MirPass for HopElision {
    fn name(&self) -> &'static str {
        "rc-hop-elision"
    }

    fn run(&self, func: &mut MirFunction, _interner: &TypeInterner) -> bool {
        let mut changed = false;
        for bi in 0..func.blocks.len() {
            let mut i = 0;
            while i < func.blocks[bi].stmts.len() {
                // Shape: Assign(n, UnionField{base}) … Retain(n) … Release(base) …
                // Union-field bind, or a niche-`Option` pointer copy (`node = curr`).
                let (n, base, plain_copy) = match &func.blocks[bi].stmts[i] {
                    Statement::Assign(
                        Place::Local(n),
                        Rvalue::UnionField {
                            base: Operand::Copy(Place::Local(b)),
                            ..
                        },
                    ) => (*n, *b, false),
                    Statement::Assign(
                        Place::Local(n),
                        Rvalue::Use(Operand::Copy(Place::Local(b))),
                    ) if *n != *b => (*n, *b, true),
                    _ => {
                        i += 1;
                        continue;
                    }
                };
                if !outside_only_releases(func, n, bi) {
                    i += 1;
                    continue;
                }
                let mut base = base;
                if plain_copy {
                    if let Some(src) = preceding_copy(&func.blocks[bi].stmts, i, base) {
                        base = src;
                    }
                }
                let Some(j) = next_matching(&func.blocks[bi].stmts[i + 1..], |s| {
                    matches!(
                        s,
                        Statement::Retain(Operand::Copy(Place::Local(l))) if *l == n
                    )
                })
                .map(|off| i + 1 + off) else {
                    i += 1;
                    continue;
                };
                let Some(k) = next_matching(&func.blocks[bi].stmts[j + 1..], |s| {
                    matches!(
                        s,
                        Statement::Release(Operand::Copy(Place::Local(l))) if *l == base
                    )
                })
                .map(|off| j + 1 + off) else {
                    i += 1;
                    continue;
                };
                // `curr = node.next` both loads the next pointer and overwrites `curr`. Split
                // the load into a temp so `Release(curr)` can sink past it and still drop the
                // old pointer.
                let split = split_field_rebind(func, bi, k, n, base);
                if plain_copy && !split {
                    i += 1;
                    continue;
                }
                let reenters = block_reenters(func, bi);
                let cancel = {
                    let block = &mut func.blocks[bi];
                    // Sink Release(base) over pure statements that do not write `n` or `base`.
                    let mut k = k;
                    while k + 1 < block.stmts.len() && sinkable(&block.stmts[k + 1], n, base) {
                        block.stmts.swap(k, k + 1);
                        k += 1;
                    }
                    // Every read of `n` must now precede the sunk release; `Release(n)` after it
                    // provides the matching half of the bracket to cancel. A loop may instead drop
                    // `n` at the head of this block, before the bind.
                    let last_read = last_read_of(&block.stmts[..k], n);
                    let rel_n = next_matching(&block.stmts[k + 1..], |s| {
                        matches!(
                            s,
                            Statement::Release(Operand::Copy(Place::Local(l2))) if *l2 == n
                        )
                    })
                    .map(|off| k + 1 + off)
                    .or_else(|| {
                        if !reenters {
                            return None;
                        }
                        block.stmts[..i].iter().rposition(|s| {
                            matches!(
                                s,
                                Statement::Release(Operand::Copy(Place::Local(l2))) if *l2 == n
                            )
                        })
                    });
                    let cancel = last_read.is_some() && rel_n.is_some();
                    if cancel {
                        block.stmts[j] = Statement::Nop;
                    }
                    cancel
                };
                if cancel {
                    // The arm binding's retain is balanced by every `Release(n)`, including a
                    // scope-exit release in a later block. Drop them all; `Release(base)` (sunk
                    // past the last read) is the one drop of the previous node.
                    for b in &mut func.blocks {
                        for s in &mut b.stmts {
                            if matches!(
                                s,
                                Statement::Release(Operand::Copy(Place::Local(l2))) if *l2 == n
                            ) {
                                *s = Statement::Nop;
                            }
                        }
                    }
                    changed = true;
                }
                i += 1;
            }
        }
        changed
    }
}

/// `base = n.field` loads the next pointer and overwrites `base` in one statement. Split it
/// into a temp load so the release of the old `base` can move past the read.
fn split_field_rebind(
    func: &mut MirFunction,
    bi: usize,
    release_at: usize,
    n: Local,
    base: Local,
) -> bool {
    let stmts = &func.blocks[bi].stmts;
    if field_then_store(stmts, release_at, n, base) {
        return true;
    }
    let Some(p) = (release_at + 1..stmts.len()).find(|&p| field_rebind(&stmts[p], n, base)) else {
        return false;
    };
    let tmp = Local(func.locals.len() as u32);
    func.locals.push(LocalDecl {
        ty: func.local_ty(base),
        name: None,
        is_ref: false,
        is_take: false,
        is_cursor: false,
        manual_drop: false,
    });
    let Statement::Assign(_, rv) = func.blocks[bi].stmts[p].clone() else {
        return false;
    };
    func.blocks[bi].stmts[p] = Statement::Assign(
        Place::Local(base),
        Rvalue::Use(Operand::Copy(Place::Local(tmp))),
    );
    func.blocks[bi].stmts.insert(p, Statement::Assign(Place::Local(tmp), rv));
    true
}

/// `tmp = n.field` followed by `base = tmp` — the load is already split from the store.
fn field_then_store(stmts: &[Statement], release_at: usize, n: Local, base: Local) -> bool {
    let mut tmp = None;
    for s in &stmts[release_at + 1..] {
        if let Statement::Assign(
            Place::Local(d),
            Rvalue::Use(Operand::Copy(Place::Field { base: f, .. })),
        ) = s
        {
            if *f == n {
                tmp = Some(*d);
            }
        }
        if let Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Copy(Place::Local(s)))) = s
        {
            if *d == base && tmp == Some(*s) {
                return true;
            }
        }
    }
    false
}

fn preceding_copy(stmts: &[Statement], bind: usize, copied: Local) -> Option<Local> {
    for s in stmts[..bind].iter().rev() {
        match s {
            Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Copy(Place::Local(src))))
                if *d == copied && *src != copied =>
            {
                return Some(*src);
            }
            Statement::Assign(
                Place::Local(d),
                Rvalue::UnionField {
                    base: Operand::Copy(Place::Local(src)),
                    ..
                },
            ) if *d == copied && *src != copied => return Some(*src),
            Statement::Assign(Place::Local(d), _) if *d == copied => return None,
            _ => {}
        }
    }
    None
}

fn outside_only_releases(func: &MirFunction, n: Local, bi: usize) -> bool {
    func.blocks.iter().enumerate().all(|(bj, b)| {
        bj == bi
            || b.stmts.iter().all(|s| {
                !super::stmt_reads_local(s, n.0)
                    || matches!(
                        s,
                        Statement::Release(Operand::Copy(Place::Local(l))) if *l == n
                    )
            })
    })
}

fn field_rebind(stmt: &Statement, n: Local, base: Local) -> bool {
    matches!(
        stmt,
        Statement::Assign(
            Place::Local(d),
            Rvalue::Use(Operand::Copy(Place::Field { base: f, .. })),
        ) if *d == base && *f == n
    )
}

fn block_reenters(func: &MirFunction, bi: usize) -> bool {
    let here = BlockId(bi as u32);
    func.blocks.iter().any(|b| terminator_targets(&b.terminator, here))
}

fn terminator_targets(term: &crate::Terminator, target: BlockId) -> bool {
    match term {
        crate::Terminator::Goto(b) => *b == target,
        crate::Terminator::If { then_blk, else_blk, .. } => *then_blk == target || *else_blk == target,
        crate::Terminator::Switch { targets, default, .. } => {
            targets.iter().any(|(_, b)| *b == target) || *default == target
        }
        _ => false,
    }
}

fn next_matching(stmts: &[Statement], pred: impl Fn(&Statement) -> bool) -> Option<usize> {
    stmts.iter().position(pred)
}

/// True when `stmt` can move below a release of the aliased object: pure, and not writing
/// `n` or `base`.
fn sinkable(stmt: &Statement, n: Local, base: Local) -> bool {
    match stmt {
        Statement::Assign(Place::Local(d), rvalue) => {
            super::is_pure_rvalue(rvalue) && *d != n && *d != base
        }
        Statement::DebugLine(_) | Statement::SourceLine(_) | Statement::Nop => true,
        _ => false,
    }
}

fn last_read_of(stmts: &[Statement], local: Local) -> Option<usize> {
    stmts
        .iter()
        .rposition(|s| super::stmt_reads_local(s, local.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use crate::Terminator;

    fn hop_mir() -> (MirFunction, crate::Local, crate::Local, crate::Local) {
        let mut i = TypeInterner::new();
        let node_ty = i.struct_ty(dream_types::DefId(7), vec![]);
        let opt_ty = i.union_ty(dream_types::DefId(8), vec![node_ty]);
        let mut b = FunctionBuilder::new("hop", i.void());
        let curr = b.new_local(opt_ty, Some("curr".into()));
        let n = b.new_temp(node_ty);
        let t = b.new_temp(node_ty);
        // n = <union-field of curr>; Retain(n); t = <union-field of n>;
        // Release(curr); curr = t; Retain(curr); Release(n)
        b.assign(
            Place::Local(n),
            Rvalue::UnionField {
                base: Operand::Copy(Place::Local(curr)),
                ty: opt_ty,
                variant: 0,
                field: 0,
            },
        );
        b.push(Statement::Retain(Operand::Copy(Place::Local(n))));
        b.assign(
            Place::Local(t),
            Rvalue::UnionField {
                base: Operand::Copy(Place::Local(n)),
                ty: opt_ty,
                variant: 0,
                field: 0,
            },
        );
        b.push(Statement::Release(Operand::Copy(Place::Local(curr))));
        b.assign(
            Place::Local(curr),
            Rvalue::Use(Operand::Copy(Place::Local(t))),
        );
        b.push(Statement::Retain(Operand::Copy(Place::Local(curr))));
        b.push(Statement::Release(Operand::Copy(Place::Local(n))));
        b.terminate(Terminator::Return(None));
        (b.finish(), curr, n, t)
    }

    #[test]
    fn cancels_bracket_around_chain_hop() {
        let i = TypeInterner::new();
        let (mut func, _curr, n, _t) = hop_mir();
        assert!(HopElision.run(&mut func, &i));
        let stmts = &func.blocks[0].stmts;
        assert!(
            !stmts.iter().any(|s| matches!(
                s,
                Statement::Retain(Operand::Copy(Place::Local(l)))
                    | Statement::Release(Operand::Copy(Place::Local(l))) if *l == n
            )),
            "borrow bracket on the arm binding must be cancelled"
        );
        // The holder's release must now sit after the extract that reads `n`.
        let rel_base = stmts
            .iter()
            .position(|s| matches!(s, Statement::Release(..)))
            .expect("base release kept");
        let extract = stmts
            .iter()
            .position(|s| matches!(s, Statement::Assign(_, Rvalue::UnionField { .. })))
            .expect("extract kept");
        assert!(extract < rel_base, "extract must precede the sunk release");
    }

    #[test]
    fn cancels_bracket_on_niche_option_hop() {
        let mut i = TypeInterner::new();
        let node_ty = i.struct_ty(dream_types::DefId(7), vec![]);
        let mut b = FunctionBuilder::new("walk", i.void());
        let curr = b.new_local(node_ty, Some("curr".into()));
        let node = b.new_temp(node_ty);
        let acc = b.new_temp(i.int());
        b.assign(
            Place::Local(node),
            Rvalue::Use(Operand::Copy(Place::Local(curr))),
        );
        b.push(Statement::Retain(Operand::Copy(Place::Local(node))));
        b.assign(
            Place::Local(acc),
            Rvalue::Use(Operand::Copy(Place::Field {
                base: node,
                field: 0,
            })),
        );
        b.push(Statement::Release(Operand::Copy(Place::Local(curr))));
        b.assign(
            Place::Local(curr),
            Rvalue::Use(Operand::Copy(Place::Field {
                base: node,
                field: 1,
            })),
        );
        b.push(Statement::Retain(Operand::Copy(Place::Local(curr))));
        b.push(Statement::Release(Operand::Copy(Place::Local(node))));
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(HopElision.run(&mut func, &i));
        let stmts = &func.blocks[0].stmts;
        assert!(
            !stmts.iter().any(|s| matches!(
                s,
                Statement::Retain(Operand::Copy(Place::Local(l)))
                    | Statement::Release(Operand::Copy(Place::Local(l))) if *l == node
            )),
            "niche hop must not retain the arm binding"
        );
        let rel = stmts
            .iter()
            .position(|s| {
                matches!(s, Statement::Release(Operand::Copy(Place::Local(l))) if *l == curr)
            })
            .expect("one release of the previous node");
        let load = stmts
            .iter()
            .position(|s| {
                matches!(
                    s,
                    Statement::Assign(_, Rvalue::Use(Operand::Copy(Place::Field { field: 1, .. })))
                )
            })
            .expect("next-pointer load");
        assert!(load < rel, "next pointer is loaded before the previous node is released");
    }

    /// `l13 = curr; release node; node = l13; retain node; …; release curr; tmp = node.next; curr = tmp`
    /// plus a scope-exit `release node` in another block. The extra copy and the early release are
    /// what niche `Option` arms lower to.
    #[test]
    fn cancels_bracket_on_copied_niche_hop() {
        let mut i = TypeInterner::new();
        let node_ty = i.struct_ty(dream_types::DefId(7), vec![]);
        let opt_ty = i.union_ty(dream_types::DefId(8), vec![node_ty]);
        let mut b = FunctionBuilder::new("walk", i.void());
        let curr = b.new_local(opt_ty, Some("curr".into()));
        let node = b.new_local(node_ty, Some("node".into()));
        let l13 = b.new_temp(node_ty);
        let acc = b.new_temp(i.int());
        let header = b.new_block();
        let arm = b.new_block();
        let exit = b.new_block();
        b.terminate(Terminator::Goto(header));
        b.switch_to(header);
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(curr)),
            then_blk: arm,
            else_blk: exit,
        });
        b.switch_to(arm);
        b.assign(
            Place::Local(l13),
            Rvalue::UnionField {
                base: Operand::Copy(Place::Local(curr)),
                ty: opt_ty,
                variant: 0,
                field: 0,
            },
        );
        b.push(Statement::Release(Operand::Copy(Place::Local(node))));
        b.assign(
            Place::Local(node),
            Rvalue::Use(Operand::Copy(Place::Local(l13))),
        );
        b.push(Statement::Retain(Operand::Copy(Place::Local(node))));
        b.assign(
            Place::Local(l13),
            Rvalue::Use(Operand::Const(crate::Const::Null)),
        );
        b.assign(
            Place::Local(acc),
            Rvalue::Use(Operand::Copy(Place::Field {
                base: node,
                field: 0,
            })),
        );
        b.push(Statement::Release(Operand::Copy(Place::Local(curr))));
        let next = b.new_temp(node_ty);
        b.assign(
            Place::Local(next),
            Rvalue::Use(Operand::Copy(Place::Field {
                base: node,
                field: 1,
            })),
        );
        b.assign(
            Place::Local(curr),
            Rvalue::Use(Operand::Copy(Place::Local(next))),
        );
        b.push(Statement::Retain(Operand::Copy(Place::Local(curr))));
        b.terminate(Terminator::Goto(header));
        b.switch_to(exit);
        b.push(Statement::Release(Operand::Copy(Place::Local(node))));
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(HopElision.run(&mut func, &i));
        let arm_stmts = &func.blocks[arm.0 as usize].stmts;
        assert!(
            !func.blocks.iter().any(|blk| blk.stmts.iter().any(|s| matches!(
                s,
                Statement::Retain(Operand::Copy(Place::Local(l)))
                    | Statement::Release(Operand::Copy(Place::Local(l))) if *l == node
            ))),
            "copied niche hop must drop every retain and release of the arm binding"
        );
        let rel = arm_stmts
            .iter()
            .position(|s| {
                matches!(s, Statement::Release(Operand::Copy(Place::Local(l))) if *l == curr)
            })
            .expect("one release of the previous node");
        let load = arm_stmts
            .iter()
            .position(|s| {
                matches!(
                    s,
                    Statement::Assign(
                        Place::Local(d),
                        Rvalue::Use(Operand::Copy(Place::Field { field: 1, .. })),
                    ) if *d == next
                )
            })
            .expect("next-pointer load");
        assert!(load < rel, "next pointer is loaded before the previous node is released");
    }

    #[test]
    fn leaves_unmatched_shapes_alone() {
        let i = TypeInterner::new();
        let (mut func, curr, n, _t) = hop_mir();
        // Remove Release(n): the bracket has no matching half, so nothing may be cancelled.
        func.blocks[0].stmts.retain(|s| {
            !matches!(
                s,
                Statement::Release(Operand::Copy(Place::Local(l))) if *l == n && *l != curr
            )
        });
        let retain_count = func.blocks[0]
            .stmts
            .iter()
            .filter(|s| matches!(s, Statement::Retain(..)))
            .count();
        HopElision.run(&mut func, &i);
        let after = func.blocks[0]
            .stmts
            .iter()
            .filter(|s| matches!(s, Statement::Retain(..)))
            .count();
        assert_eq!(retain_count, after, "no retain may vanish without its pair");
    }
}
