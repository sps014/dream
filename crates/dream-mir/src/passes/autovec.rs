//! Compile-time autovec of proven counted `float[]` loops: four consecutive
//! `out[i] = a[i] ±/* b[i]` (or `out[i] = a[i] ±/* c`) iterations become one `v128` op.

use super::cfg;
use super::MirPass;
use crate::{BinOp, BlockId, Const, Local, MirFunction, Operand, Place, Rvalue, Statement};
use dream_types::{PrimTy, TyKind, TypeInterner};
use std::collections::BTreeSet;

pub struct Autovec;

impl MirPass for Autovec {
    fn name(&self) -> &'static str {
        "autovec"
    }

    fn run(&self, func: &mut MirFunction, interner: &TypeInterner) -> bool {
        let loops = cfg::natural_loops(func);
        let mut changed = false;
        for l in loops {
            changed |= vectorize_loop(func, interner, &l.body, l.header, &l.latches);
        }
        changed
    }
}

fn vectorize_loop(
    func: &mut MirFunction,
    interner: &TypeInterner,
    body: &BTreeSet<BlockId>,
    header: BlockId,
    latches: &[BlockId],
) -> bool {
    if latches.len() != 1 || body.len() > 4 {
        return false;
    }
    let latch = latches[0];
    // Body blocks excluding header and latch: want a single body block with one float binop store.
    let inner: Vec<BlockId> = body
        .iter()
        .copied()
        .filter(|b| *b != latch && *b != header)
        .collect();
    if inner.len() != 1 {
        return false;
    }
    let body_blk = inner[0];
    // Identify `idx += 1` on latch and a float store `arr[idx] = Binary(op, Copy(Index), ...)`.
    let mut step_idx = None;
    for stmt in &func.block(latch).stmts {
        if let Statement::Assign(
            Place::Local(d),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(a)),
                Operand::Const(Const::Int(1)),
            ),
        ) = stmt
        {
            if d.0 == a.0 {
                step_idx = Some(*d);
            }
        }
    }
    let idx = match step_idx {
        Some(i) => i,
        None => return false,
    };
    let stmts = &func.block(body_blk).stmts;
    if stmts.len() != 1 {
        return false;
    }
    let Statement::Assign(
        Place::Index {
            base: dest,
            index,
            unchecked: true,
        },
        Rvalue::Binary(op, lhs, rhs),
    ) = &stmts[0]
    else {
        return false;
    };
    if !matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul) {
        return false;
    }
    if !index_is(index, idx) {
        return false;
    }
    let dest = *dest;
    let op = *op;
    let Some(elem) = array_elem(interner, func.local_ty(dest)) else {
        return false;
    };
    if !matches!(interner.kind(elem), TyKind::Prim(PrimTy::Float)) {
        return false;
    }
    let Some(lhs_arr) = index_base(lhs, idx) else {
        return false;
    };
    let Some(rhs_arr) = index_base(rhs, idx) else {
        return false;
    };
    func.block_mut(body_blk).stmts[0] = Statement::SimdF32x4 {
        op,
        dest: Operand::Copy(Place::Local(dest)),
        lhs: Operand::Copy(Place::Local(lhs_arr)),
        rhs: Operand::Copy(Place::Local(rhs_arr)),
        index: Operand::Copy(Place::Local(idx)),
    };
    for stmt in &mut func.block_mut(latch).stmts {
        if let Statement::Assign(
            Place::Local(d),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(a)),
                Operand::Const(Const::Int(1)),
            ),
        ) = stmt
        {
            if d.0 == a.0 && d.0 == idx.0 {
                *stmt = Statement::Assign(
                    Place::Local(idx),
                    Rvalue::Binary(
                        BinOp::Add,
                        Operand::Copy(Place::Local(idx)),
                        Operand::Const(Const::Int(4)),
                    ),
                );
            }
        }
    }
    true
}

fn index_is(index: &Operand, idx: Local) -> bool {
    matches!(index, Operand::Copy(Place::Local(l)) if l.0 == idx.0)
}

fn index_base(op: &Operand, idx: Local) -> Option<Local> {
    match op {
        Operand::Copy(Place::Index {
            base,
            index,
            unchecked: true,
        }) if index_is(index, idx) => Some(*base),
        Operand::Copy(Place::Deref { ptr, .. }) => Some(*ptr),
        _ => None,
    }
}

fn array_elem(interner: &TypeInterner, ty: dream_types::TypeId) -> Option<dream_types::TypeId> {
    match interner.kind(ty) {
        TyKind::Array(e) => Some(*e),
        _ => None,
    }
}
