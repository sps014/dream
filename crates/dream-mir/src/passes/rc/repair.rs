//! Last-use move repair after inlining.
//!
//! [`super::RcInsertion`] runs *before* inlining so callee size (and destruction timing) stay
//! stable. The inliner then splices `a[i] = s` into a larger CFG where `s` is dead — but a baked
//! `Retain(s)` / missing `s = null` stay. Re-running insertion on fused `generated_dispatch` is too
//! expensive. This pass is linear and only rewrites last-use **index** stores of hidden-borrow
//! types: null the source and drop a share-`Retain` into that store. Field stores stay on
//! [`super::RcInsertion`].

use super::liveness::{self, live_after_stmt};
use super::tokens::{is_hidden_borrow_ty, is_owned_local};
use super::uniqueness::container_store_src;
use crate::passes::MirPass;
use crate::{Const, Local, MirFunction, Operand, Place, Rvalue, Statement};
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

fn repair(func: &mut MirFunction, interner: &TypeInterner) -> bool {
    let nloc = func.locals.len();
    if nloc == 0 {
        return false;
    }
    let live_out = liveness::live_out(func);
    let mut changed = false;
    for bi in 0..func.blocks.len() {
        let mut si = 0;
        while si < func.blocks[bi].stmts.len() {
            let stmt = &func.blocks[bi].stmts[si];
            let src = match stmt {
                Statement::Assign(Place::Index { .. }, _) => container_store_src(stmt).filter(|&src| {
                    is_owned_local(func, interner, src)
                        && is_hidden_borrow_ty(func, interner, src)
                        && !live_after_stmt(func, &live_out, bi, si, src)
                }),
                _ => None,
            };
            let Some(src) = src else {
                si += 1;
                continue;
            };
            if si > 0
                && matches!(
                    &func.blocks[bi].stmts[si - 1],
                    Statement::Retain(Operand::Copy(Place::Local(l))) if l.0 == src
                )
            {
                func.blocks[bi].stmts.remove(si - 1);
                si -= 1;
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
            retains,
            0,
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
