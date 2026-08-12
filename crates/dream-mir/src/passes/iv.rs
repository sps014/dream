//! Induction-variable strength reduction: `arr[idx]` in a counted `idx += 1` loop becomes a bump
//! pointer ([`Place::Deref`]) so emit does not recompute `base + 4 + idx * esize` each iteration.

use super::cfg;
use super::MirPass;
use crate::{
    BinOp, BlockId, Const, Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator,
};
use dream_hir::scalar_size;
use dream_types::{TyKind, TypeId, TypeInterner};
use std::collections::BTreeSet;

pub struct IvCanon;

impl MirPass for IvCanon {
    fn name(&self) -> &'static str {
        "iv"
    }

    fn run(&self, func: &mut MirFunction, interner: &TypeInterner) -> bool {
        let loops = cfg::natural_loops(func);
        let mut changed = false;
        for l in loops {
            changed |= rewrite_loop(func, interner, &l.body, l.header, &l.latches);
        }
        changed
    }
}

fn rewrite_loop(
    func: &mut MirFunction,
    interner: &TypeInterner,
    body: &BTreeSet<BlockId>,
    header: BlockId,
    latches: &[BlockId],
) -> bool {
    if latches.len() != 1 {
        return false;
    }
    let latch = latches[0];
    let Some((idx, arr, elem_ty)) = find_iv_index(func, interner, body, latch) else {
        return false;
    };
    if interner.is_value_type(elem_ty) {
        return false;
    }
    let (esize, _) = scalar_size(interner, elem_ty);
    if esize == 0 {
        return false;
    }

    let ptr = new_int_temp(func, interner);
    let scaled = new_int_temp(func, interner);
    let ph = BlockId(func.blocks.len() as u32);
    func.blocks.push(crate::BasicBlock {
        stmts: vec![
            Statement::Assign(
                Place::Local(scaled),
                Rvalue::Binary(
                    BinOp::Mul,
                    Operand::Copy(Place::Local(idx)),
                    Operand::Const(Const::Int(esize as i64)),
                ),
            ),
            Statement::Assign(
                Place::Local(ptr),
                Rvalue::Binary(
                    BinOp::Add,
                    Operand::Copy(Place::Local(arr)),
                    Operand::Const(Const::Int(4)),
                ),
            ),
            Statement::Assign(
                Place::Local(ptr),
                Rvalue::Binary(
                    BinOp::Add,
                    Operand::Copy(Place::Local(ptr)),
                    Operand::Copy(Place::Local(scaled)),
                ),
            ),
        ],
        terminator: Terminator::Goto(header),
    });
    redirect_header_entries(func, body, header, ph);
    replace_indices(func, body, idx, arr, ptr, elem_ty);
    func.block_mut(latch).stmts.push(Statement::Assign(
        Place::Local(ptr),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(ptr)),
            Operand::Const(Const::Int(esize as i64)),
        ),
    ));
    true
}

fn new_int_temp(func: &mut MirFunction, interner: &TypeInterner) -> Local {
    let id = Local(func.locals.len() as u32);
    func.locals.push(crate::LocalDecl {
        ty: interner.int(),
        name: None,
        is_ref: false,
        is_take: false,
        is_cursor: false,
        manual_drop: false,
    });
    id
}

fn find_iv_index(
    func: &MirFunction,
    interner: &TypeInterner,
    body: &BTreeSet<BlockId>,
    latch: BlockId,
) -> Option<(Local, Local, TypeId)> {
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
    let idx = step_idx?;
    let mut found = None;
    for &b in body {
        for stmt in &func.block(b).stmts {
            match stmt {
                Statement::Assign(
                    _,
                    Rvalue::Use(Operand::Copy(Place::Index {
                        base,
                        index,
                        unchecked: true,
                    })),
                )
                | Statement::Assign(
                    Place::Index {
                        base,
                        index,
                        unchecked: true,
                    },
                    _,
                ) => {
                    if let Operand::Copy(Place::Local(i)) = index.as_ref() {
                        if i.0 == idx.0 {
                            if let TyKind::Array(elem) = interner.kind(func.local_ty(*base)) {
                                found = Some((idx, *base, *elem));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    found
}

fn redirect_header_entries(
    func: &mut MirFunction,
    body: &BTreeSet<BlockId>,
    header: BlockId,
    ph: BlockId,
) {
    let n = func.blocks.len();
    for i in 0..n {
        let bid = BlockId(i as u32);
        if body.contains(&bid) || bid == ph {
            continue;
        }
        retarget(&mut func.blocks[i].terminator, header, ph);
    }
}

fn retarget(t: &mut Terminator, from: BlockId, to: BlockId) {
    match t {
        Terminator::Goto(b) if *b == from => *b = to,
        Terminator::If {
            then_blk, else_blk, ..
        } => {
            if *then_blk == from {
                *then_blk = to;
            }
            if *else_blk == from {
                *else_blk = to;
            }
        }
        Terminator::Switch { targets, default, .. } => {
            for (_, b) in targets {
                if *b == from {
                    *b = to;
                }
            }
            if *default == from {
                *default = to;
            }
        }
        Terminator::Await { resume, .. } if *resume == from => *resume = to,
        _ => {}
    }
}

fn replace_indices(
    func: &mut MirFunction,
    body: &BTreeSet<BlockId>,
    idx: Local,
    arr: Local,
    ptr: Local,
    elem_ty: TypeId,
) {
    for &b in body {
        for stmt in &mut func.block_mut(b).stmts {
            if let Statement::Assign(place, rv) = stmt {
                replace_place(place, idx, arr, ptr, elem_ty);
                if let Rvalue::Use(Operand::Copy(p)) = rv {
                    replace_place(p, idx, arr, ptr, elem_ty);
                }
            }
        }
    }
}

fn replace_place(place: &mut Place, idx: Local, arr: Local, ptr: Local, elem_ty: TypeId) {
    if let Place::Index {
        base,
        index,
        unchecked: true,
    } = place
    {
        if base.0 == arr.0 {
            if let Operand::Copy(Place::Local(i)) = index.as_ref() {
                if i.0 == idx.0 {
                    *place = Place::Deref { ptr, elem_ty };
                }
            }
        }
    }
}
