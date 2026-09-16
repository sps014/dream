//! Last-use move repair after inlining.
//!
//! [`super::RcInsertion`] runs *before* inlining so callee size (and destruction timing) stay
//! stable. The inliner then splices `a[i] = s` into a larger CFG where `s` is dead — but a baked
//! `Retain(s)` / missing `s = null` stay. Re-running insertion on fused `generated_dispatch` is too
//! expensive. This pass is linear and only rewrites last-use **index** stores of owned RC
//! sources: null the source and drop a share-`Retain` into that store. Field stores stay on
//! [`super::RcInsertion`].

use super::liveness::{self, live_after_stmt};
use super::tokens::is_owned_local;
use super::uniqueness::{can_container_move, container_store_src, fresh_locals, mark_container_move};
use crate::passes::MirPass;
use crate::{Const, Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;

pub struct RcLastUseRepair;

impl MirPass for RcLastUseRepair {
    fn name(&self) -> &'static str {
        "rc-last-use-repair"
    }

    fn run(&self, func: &mut MirFunction, interner: &TypeInterner) -> bool {
        repair(func, interner)
    }
}

/// Location of the `Retain` that feeds the index store at `(bi, si)`, if one was baked in ahead of
/// it.
///
/// An index store retains for itself, so the value reaching it must be a borrow. Inlining a
/// returning callee leaves behind both the callee's `Retain` — the `+1` a real return would have
/// handed back — and a `dest = <returned local>` copy of its result, and the two land on opposite
/// sides of the splice, so the retain is neither the previous statement nor necessarily in this
/// block. Walk back through that copy, crossing at most the one `goto` edge the splice leaves, and
/// only while the copy source is dead after the store: a still-live alias owns the reference, and
/// dropping the retain would free it early.
///
/// Looking through a copy also demands that the retained local was read out of a container, so the
/// `Retain` is provably the inlined return's `+1` and not the only reference to a fresh value.
fn baked_retain(
    func: &MirFunction,
    live_out: &[std::collections::HashSet<u32>],
    bi: usize,
    si: usize,
    src: u32,
    interner: &TypeInterner,
) -> Option<(usize, usize)> {
    let mut alias = src;
    let mut copied = false;
    let mut block = bi;
    let mut end = si;
    loop {
        for ri in (0..end).rev() {
            match &func.blocks[block].stmts[ri] {
                Statement::Retain(Operand::Copy(Place::Local(l))) if l.0 == alias => {
                    if copied && !defines_borrow(&func.blocks[block].stmts[..ri], alias) {
                        return None;
                    }
                    return Some((block, ri));
                }
                Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Copy(Place::Local(s))))
                    if d.0 == alias =>
                {
                    if !is_owned_local(func, interner, s.0)
                        || live_after_stmt(func, live_out, bi, si, s.0)
                    {
                        return None;
                    }
                    alias = s.0;
                    copied = true;
                }
                _ => return None,
            }
        }
        if block != bi {
            return None;
        }
        block = sole_goto_pred(func, bi)?;
        end = func.blocks[block].stmts.len();
    }
}

/// The unique predecessor of `bi` when it reaches `bi` by a plain `goto` — the shape inlining
/// leaves between a spliced callee body and the continuation holding the call's destination.
fn sole_goto_pred(func: &MirFunction, bi: usize) -> Option<usize> {
    let mut found = None;
    for (pi, block) in func.blocks.iter().enumerate() {
        if !block
            .terminator
            .successors()
            .iter()
            .any(|s| s.0 as usize == bi)
        {
            continue;
        }
        if found.is_some() || !matches!(block.terminator, Terminator::Goto(_)) {
            return None;
        }
        found = Some(pi);
    }
    found
}

/// Whether the last statement in `stmts` defining `local` reads it out of a container, which makes
/// it a borrow that owns nothing of its own.
fn defines_borrow(stmts: &[Statement], local: u32) -> bool {
    stmts.iter().rev().find_map(|st| match st {
        Statement::Assign(Place::Local(d), rv) if d.0 == local => Some(matches!(
            rv,
            Rvalue::Use(Operand::Copy(Place::Index { .. } | Place::Field { .. }))
        )),
        _ => None,
    }) == Some(true)
}

fn repair(func: &mut MirFunction, interner: &TypeInterner) -> bool {
    let nloc = func.locals.len();
    if nloc == 0 {
        return false;
    }
    let live_out = liveness::live_out(func);
    let fresh = fresh_locals(func, interner);
    let mut changed = false;
    for bi in 0..func.blocks.len() {
        let mut si = 0;
        while si < func.blocks[bi].stmts.len() {
            let stmt = &func.blocks[bi].stmts[si];
            let src = match stmt {
                Statement::Assign(Place::Index { .. }, _) => {
                    container_store_src(stmt).filter(|&src| {
                        is_owned_local(func, interner, src)
                            && can_container_move(interner, func.locals[src as usize].ty)
                            && !live_after_stmt(func, &live_out, bi, si, src)
                    })
                }
                _ => None,
            };
            let Some(src) = src else {
                si += 1;
                continue;
            };
            if let Some((rb, ri)) = baked_retain(func, &live_out, bi, si, src, interner) {
                // The stripped `Retain` is replaced by the one the store makes for itself, so the
                // slot still ends up holding a reference of its own and the source's is given up by
                // the null below.
                func.blocks[rb].stmts.remove(ri);
                if rb == bi {
                    si -= 1;
                }
                changed = true;
            } else if fresh[src as usize] {
                // Nothing retained on this store's behalf and the source is freshly allocated, so
                // the store adopts that `+1`. Recording it on the statement, before the per-function
                // pipeline runs, is what stops copy propagation from rewriting the store's operand
                // and splitting it from the null below — which would leave the slot retaining a
                // second reference while the null still discarded the first.
                mark_container_move(&mut func.blocks[bi].stmts[si], src);
                changed = true;
            } else {
                // The store retains for itself and nothing was stripped to pay for it, so the
                // reference the source came in with is still its own and this is its last use. The
                // null below ends its life without giving that reference up, so give it up here.
                func.blocks[bi].stmts.insert(
                    si + 1,
                    Statement::Release(Operand::Copy(Place::Local(Local(src)))),
                );
                si += 1;
                changed = true;
            }
            let already_null = func.blocks[bi].stmts.get(si + 1).is_some_and(|n| {
                matches!(
                    n,
                    Statement::Assign(Place::Local(l), Rvalue::Use(Operand::Const(Const::Null)))
                        if l.0 == src
                )
            });
            if !already_null {
                func.blocks[bi].stmts.insert(
                    si + 1,
                    Statement::Assign(
                        Place::Local(Local(src)),
                        Rvalue::Use(Operand::Const(Const::Null)),
                    ),
                );
                changed = true;
                si += 2;
            } else {
                si += 1;
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use crate::{Operand, Place, Rvalue, Terminator};

    #[test]
    fn strips_retain_and_nulls_last_use_string_index_store() {
        let mut ctx = dream_types::TypeCtx::new();
        let str_ty = ctx.interner.string();
        let arr_ty = ctx.interner.array(str_ty);
        let mut b = FunctionBuilder::new("f", ctx.interner.void());
        let arr = b.new_local(arr_ty, Some("arr".into()));
        let s = b.new_local(str_ty, Some("s".into()));
        b.assign(
            Place::Local(arr),
            Rvalue::ArrayNew {
                elem_ty: str_ty,
                len: Operand::Const(crate::Const::Int(1)),
            },
        );
        b.assign(
            Place::Local(s),
            Rvalue::Use(Operand::Const(crate::Const::Str("x".into()))),
        );
        b.push(Statement::Retain(Operand::Copy(Place::Local(s))));
        b.assign(
            Place::index(arr, Operand::Const(crate::Const::Int(0))),
            Rvalue::Use(Operand::Copy(Place::Local(s))),
        );
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        assert!(RcLastUseRepair.run(&mut func, &ctx.interner));
        let stmts = &func.blocks[0].stmts;
        let retains = stmts
            .iter()
            .filter(|st| matches!(st, Statement::Retain(Operand::Copy(Place::Local(l))) if *l == s))
            .count();
        assert_eq!(
            retains, 0,
            "last-use store must not keep a share Retain: {:?}",
            stmts
        );
        let null_s = stmts.iter().any(|st| {
            matches!(
                st,
                Statement::Assign(Place::Local(l), Rvalue::Use(Operand::Const(crate::Const::Null)))
                    if *l == s
            )
        });
        assert!(null_s, "source nulled: {:?}", stmts);
    }
}
