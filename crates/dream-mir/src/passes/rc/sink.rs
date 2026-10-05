//! Release sinking so string rebuilds reuse their buffer.
//!
//! A loop whose body rebinds a string (`let s = a + b;` per iteration) ends each iteration by
//! dropping the old value in the loop header: `Release(s); s = null;` ahead of the exit test. The
//! body's `tmp = concat(..); Release(s); s = tmp` then always sees `s == null`, so the `_into`
//! runtime entry (which rebuilds in place when the old block is uniquely owned) never gets a block
//! to reuse.
//!
//! Two rewrites fix that without changing what is released or how often:
//!
//! 1. **Sink.** A trailing `Release(s); s = null` followed only by pure, `s`-free statements and a
//!    terminator that does not read `s` moves to the head of every successor. Only pure statements
//!    are skipped over, so nothing observable runs between the old and new position. Each
//!    successor must have this block as its only predecessor (so every path still releases once),
//!    and the successors must be distinct.
//! 2. **Merge.** In a string slot, `Release(s); s = null; X; Release(s)` with `X` neither reading
//!    nor writing `s` drops the first pair: the second release (a no-op on null before) now drops
//!    the old value. Strings have no destructor, so freeing the block after `X` rather than before
//!    is unobservable. Region, defer and force-free statements in `X` block the merge, since they
//!    change how a release is carried out.

use super::is_transparent_stmt;
use super::liveness::{add_terminator_reads, stmt_reads_local};
use crate::passes::MirPass;
use crate::{BlockId, Const, Local, MirFunction, Operand, Place, Rvalue, Statement};
use dream_types::TypeInterner;
use indexmap::IndexSet;

pub struct ReleaseSink;

impl MirPass for ReleaseSink {
    fn preserves(&self) -> crate::passes::PreservedAnalyses {
        crate::passes::PreservedAnalyses::ControlFlow
    }

    fn name(&self) -> &'static str {
        "release-sink"
    }

    fn transform(
        &self,
        func: &mut MirFunction,
        interner: &TypeInterner,
        _layouts: &dream_hir::LayoutTable,
        analyses: &mut crate::passes::FunctionAnalyses,
    ) -> bool {
        let mut changed = false;
        // Each sink moves a pair one edge forward; the cap only matters for an unreachable cycle
        // of single-predecessor blocks, where the pair could circulate forever.
        let mut budget = func.blocks.len() * 4;
        while budget > 0 && sink_one(func, analyses) {
            budget -= 1;
            changed = true;
        }
        for bi in 0..func.blocks.len() {
            while merge_one(func, interner, bi) {
                changed = true;
            }
        }
        changed
    }
}

fn released_local(stmt: &Statement) -> Option<Local> {
    match stmt {
        Statement::Release(Operand::Copy(Place::Local(l))) => Some(*l),
        _ => None,
    }
}

fn is_null_store(stmt: &Statement, local: Local) -> bool {
    matches!(
        stmt,
        Statement::Assign(Place::Local(l), Rvalue::Use(Operand::Const(Const::Null))) if *l == local
    )
}

fn writes_local(stmt: &Statement, local: Local) -> bool {
    let base = match stmt {
        Statement::Assign(Place::Local(l), _) => Some(*l),
        Statement::Assign(Place::Field { base, .. }, _)
        | Statement::Assign(Place::Index { base, .. }, _) => Some(*base),
        Statement::Assign(Place::Deref { ptr, .. }, _) => Some(*ptr),
        Statement::ValueDrop(l) | Statement::ValueRetain(l) | Statement::ValueKill(l) => Some(*l),
        _ => None,
    };
    base == Some(local)
}

fn touches(stmt: &Statement, local: Local) -> bool {
    stmt_reads_local(stmt, local.0) || writes_local(stmt, local)
}

/// `(block, index of the Release)` of the first sinkable trailing pair.
fn sink_candidate(func: &MirFunction, preds: &[Vec<BlockId>]) -> Option<(usize, usize)> {
    for (bi, block) in func.blocks.iter().enumerate() {
        let succs = block.terminator.successors();
        if succs.is_empty() {
            continue;
        }
        let distinct: IndexSet<BlockId> = succs.iter().copied().collect();
        if distinct.len() != succs.len()
            || succs
                .iter()
                .any(|s| s.0 as usize == bi || preds[s.0 as usize].len() != 1)
        {
            continue;
        }
        let stmts = &block.stmts;
        for si in (0..stmts.len().saturating_sub(1)).rev() {
            let Some(s) = released_local(&stmts[si]) else {
                continue;
            };
            if !is_null_store(&stmts[si + 1], s) {
                continue;
            }
            let tail_ok = stmts[si + 2..]
                .iter()
                .all(|t| is_transparent_stmt(t) && !touches(t, s));
            let mut term_reads = IndexSet::new();
            add_terminator_reads(&block.terminator, &mut term_reads);
            if tail_ok && !term_reads.contains(&s.0) {
                return Some((bi, si));
            }
            break;
        }
    }
    None
}

fn sink_one(func: &mut MirFunction, analyses: &mut crate::passes::FunctionAnalyses) -> bool {
    let preds = analyses.predecessors(func);
    let Some((bi, si)) = sink_candidate(func, &preds) else {
        return false;
    };
    let pair: Vec<Statement> = func.blocks[bi].stmts.drain(si..si + 2).collect();
    for s in func.blocks[bi].terminator.successors() {
        let head = &mut func.blocks[s.0 as usize].stmts;
        head.splice(0..0, pair.iter().cloned());
    }
    true
}

fn blocks_merge(stmt: &Statement) -> bool {
    matches!(
        stmt,
        Statement::RegionEnter
            | Statement::RegionLeave
            | Statement::DeferEnter
            | Statement::DeferLeave(_)
            | Statement::ForceFree(_)
            | Statement::Retain(_)
            | Statement::Release(_)
    )
}

fn merge_one(func: &mut MirFunction, interner: &TypeInterner, bi: usize) -> bool {
    let string = interner.string();
    let stmts = &func.blocks[bi].stmts;
    for si in 0..stmts.len().saturating_sub(1) {
        let Some(s) = released_local(&stmts[si]) else {
            continue;
        };
        if func.local_ty(s) != string || !is_null_store(&stmts[si + 1], s) {
            continue;
        }
        let mut j = si + 2;
        while j < stmts.len() && released_local(&stmts[j]) != Some(s) {
            if touches(&stmts[j], s) || blocks_merge(&stmts[j]) {
                break;
            }
            j += 1;
        }
        if j < stmts.len() && released_local(&stmts[j]) == Some(s) {
            func.blocks[bi].stmts.drain(si..si + 2);
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use crate::{BinOp, Terminator};

    fn rel(l: Local) -> Statement {
        Statement::Release(Operand::Copy(Place::Local(l)))
    }

    fn null(l: Local) -> Statement {
        Statement::Assign(Place::Local(l), Rvalue::Use(Operand::Const(Const::Null)))
    }

    /// `header: Release(s); s = null; c = i < n; if c body else exit`,
    /// `body: tmp = s' ; Release(s); s = tmp; goto header`.
    fn loop_fn(i: &TypeInterner, s_ty: dream_types::TypeId) -> (MirFunction, Local) {
        let mut b = FunctionBuilder::new("f", i.void());
        let s = b.new_local(s_ty, Some("s".into()));
        let tmp = b.new_local(s_ty, None);
        let c = b.new_local(i.bool(), None);
        let n = b.new_local(i.int(), None);
        let header = b.new_block();
        let body = b.new_block();
        let exit = b.new_block();
        b.terminate(Terminator::Goto(header));
        b.switch_to(header);
        b.push(rel(s));
        b.push(null(s));
        b.assign(
            Place::Local(c),
            Rvalue::Binary(
                BinOp::Lt,
                Operand::Const(Const::Int(0)),
                Operand::Copy(Place::Local(n)),
            ),
        );
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(c)),
            then_blk: body,
            else_blk: exit,
        });
        b.switch_to(body);
        b.assign(
            Place::Local(tmp),
            Rvalue::Concat(vec![
                Operand::Const(Const::Str("a".into())),
                Operand::Const(Const::Str("b".into())),
            ]),
        );
        b.push(rel(s));
        b.assign(
            Place::Local(s),
            Rvalue::Use(Operand::Copy(Place::Local(tmp))),
        );
        b.terminate(Terminator::Goto(header));
        b.switch_to(exit);
        b.terminate(Terminator::Return(None));
        (b.finish(), s)
    }

    #[test]
    fn sinks_header_release_so_the_body_rebuild_sees_the_old_string() {
        let i = TypeInterner::new();
        let (mut f, s) = loop_fn(&i, i.string());
        assert!(ReleaseSink.run(&mut f, &i));
        let header = &f.blocks[1].stmts;
        assert!(header.iter().all(|st| released_local(st) != Some(s)));
        let body = &f.blocks[2].stmts;
        assert!(matches!(body[0], Statement::Assign(..)));
        assert_eq!(released_local(&body[1]), Some(s));
        assert_eq!(
            body.iter()
                .filter(|st| released_local(st) == Some(s))
                .count(),
            1
        );
        let exit = &f.blocks[3].stmts;
        assert_eq!(released_local(&exit[0]), Some(s));
        assert!(is_null_store(&exit[1], s));
    }

    #[test]
    fn keeps_both_releases_for_non_string_slots() {
        let mut i = TypeInterner::new();
        let int = i.int();
        let arr = i.array(int);
        let (mut f, s) = loop_fn(&i, arr);
        ReleaseSink.run(&mut f, &i);
        let body = &f.blocks[2].stmts;
        assert_eq!(
            body.iter()
                .filter(|st| released_local(st) == Some(s))
                .count(),
            2
        );
    }

    #[test]
    fn does_not_sink_into_a_shared_successor() {
        let i = TypeInterner::new();
        let (mut f, s) = loop_fn(&i, i.string());
        f.blocks[0].terminator = Terminator::If {
            cond: Operand::Const(Const::Bool(true)),
            then_blk: BlockId(1),
            else_blk: BlockId(3),
        };
        ReleaseSink.run(&mut f, &i);
        assert_eq!(released_local(&f.blocks[1].stmts[0]), Some(s));
    }
}
