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
use crate::{Local, MirFunction, Operand, Place, Rvalue, Statement};
use dream_types::TypeInterner;
use std::collections::HashSet;

pub struct HopElision;

impl MirPass for HopElision {
    fn name(&self) -> &'static str {
        "rc-hop-elision"
    }

    fn run(&self, func: &mut MirFunction, _interner: &TypeInterner) -> bool {
        let mut changed = false;
        // Pre-compute per-local confinement: a candidate binding must be read only inside its
        // own block, or cancelling its bracket could dangle uses elsewhere.
        let mut blocks_reading: HashSet<(u32, usize)> = HashSet::new();
        for (bi, b) in func.blocks.iter().enumerate() {
            for s in &b.stmts {
                for l in stmt_read_locals(s) {
                    blocks_reading.insert((l, bi));
                }
            }
        }
        for bi in 0..func.blocks.len() {
            let block = &mut func.blocks[bi];
            let mut i = 0;
            while i < block.stmts.len() {
                // Shape: Assign(n, UnionField{base}) … Retain(n) … Release(base) …
                let (n, base) = match &block.stmts[i] {
                    Statement::Assign(
                        Place::Local(n),
                        Rvalue::UnionField {
                            base: Operand::Copy(Place::Local(b)),
                            ..
                        },
                    ) => (*n, *b),
                    _ => {
                        i += 1;
                        continue;
                    }
                };
                if blocks_reading.iter().any(|&(l, bj)| l == n.0 && bj != bi) {
                    i += 1;
                    continue;
                }
                let Some(j) = next_matching(&block.stmts[i + 1..], |s| {
                    matches!(
                        s,
                        Statement::Retain(Operand::Copy(Place::Local(l))) if *l == n
                    )
                })
                .map(|off| i + 1 + off) else {
                    i += 1;
                    continue;
                };
                let Some(k) = next_matching(&block.stmts[j + 1..], |s| {
                    matches!(
                        s,
                        Statement::Release(Operand::Copy(Place::Local(l))) if *l == base
                    )
                })
                .map(|off| j + 1 + off) else {
                    i += 1;
                    continue;
                };
                // Sink Release(base) over pure statements that do not write `n` or `base`.
                let mut k = k;
                while k + 1 < block.stmts.len() && sinkable(&block.stmts[k + 1], n, base) {
                    block.stmts.swap(k, k + 1);
                    k += 1;
                }
                // Every read of `n` must now precede the sunk release; `Release(n)` after it
                // provides the matching half of the bracket to cancel.
                let last_read = last_read_of(&block.stmts[..k], n);
                let rel_n = next_matching(&block.stmts[k + 1..], |s| {
                    matches!(
                        s,
                        Statement::Release(Operand::Copy(Place::Local(l2))) if *l2 == n
                    )
                })
                .map(|off| k + 1 + off);
                if let (Some(_), Some(l)) = (last_read, rel_n) {
                    block.stmts[j] = Statement::Nop;
                    block.stmts[l] = Statement::Nop;
                    changed = true;
                }
                i += 1;
            }
        }
        changed
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

/// Locals a statement reads (union-field bases and plain operand/base locals).
fn stmt_read_locals(stmt: &Statement) -> Vec<u32> {
    let mut out = Vec::new();
    let place_local = |p: &Place| -> Option<u32> {
        match p {
            Place::Local(l) => Some(l.0),
            Place::Field { base, .. } | Place::Index { base, .. } => Some(base.0),
            Place::Deref { ptr, .. } => Some(ptr.0),
            Place::Global(_) => None,
        }
    };
    let op_local = |op: &Operand| match op {
        Operand::Copy(p) => place_local(p),
        Operand::Const(_) => None,
    };
    match stmt {
        Statement::Assign(place, rv) => {
            for op in rvalue_operands(rv) {
                if let Some(l) = op_local(op) {
                    out.push(l);
                }
            }
            if let Some(l) = place_local(place) {
                out.push(l);
            }
        }
        Statement::Release(op) | Statement::Retain(op) => out.extend(op_local(op)),
        _ => {}
    }
    out
}

fn rvalue_operands(rv: &Rvalue) -> Vec<&Operand> {
    let mut out = Vec::new();
    match rv {
        Rvalue::Use(o) | Rvalue::Unary(_, o) => out.push(o),
        Rvalue::Binary(_, a, b) => {
            out.push(a);
            out.push(b);
        }
        Rvalue::UnionField { base, .. } => out.push(base),
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => {
            out.push(cond);
            out.push(then_val);
            out.push(else_val);
        }
        _ => {}
    }
    out
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
