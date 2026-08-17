//! Remainder-safe autovec of counted `T[]` loops onto WASM `v128`.
//!
//! SIMD chunks run only while `idx + L <= n` (`L = 16/sizeof(T)`). The original scalar body
//! covers the tail so a trip count that is not a multiple of `L` cannot overrun. The pass still
//! requires ABC `unchecked` indexes (or IV bump pointers derived from them).

use super::cfg;
use super::MirPass;
use crate::{
    BinOp, BlockId, Const, Local, LocalDecl, MirFunction, Operand, Place, Rvalue, SimdLane,
    Statement, Terminator,
};
use dream_types::{TyKind, TypeInterner};
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

struct Candidate {
    stmt_idx: usize,
    in_latch: bool,
    body_blk: BlockId,
    op: BinOp,
    lane: SimdLane,
    dest: Local,
    lhs: Operand,
    rhs: Operand,
    splat_rhs: Option<Operand>,
    ptr_addr: bool,
    dest_ptr: Option<Local>,
    lhs_ptr: Option<Local>,
    rhs_ptr: Option<Local>,
    elem_ty: dream_types::TypeId,
}

fn vectorize_loop(
    func: &mut MirFunction,
    interner: &TypeInterner,
    body: &BTreeSet<BlockId>,
    header: BlockId,
    latches: &[BlockId],
) -> bool {
    if latches.len() != 1 || body.len() > 8 {
        return false;
    }
    let latch = latches[0];
    let idx = match step_local(func, latch) {
        Some(i) => i,
        None => return false,
    };
    let Some(n_bound) = header_bound(func, header, idx) else {
        return false;
    };
    let orig_else = match func.block(header).terminator {
        Terminator::If { else_blk, .. } => else_blk,
        _ => return false,
    };
    let orig_then = match func.block(header).terminator {
        Terminator::If { then_blk, .. } => then_blk,
        _ => return false,
    };

    let cand = match find_candidate(func, interner, body, header, latch, idx) {
        Some(c) => c,
        None => return false,
    };
    if !cand.lane.supports_binop(cand.op) {
        return false;
    }
    let l = cand.lane.count();
    let esize = cand.lane.esize() as i64;

    let simd_stmt = Statement::SimdV128 {
        lane: cand.lane,
        op: cand.op,
        dest: Operand::Copy(Place::Local(cand.dest)),
        lhs: cand.lhs.clone(),
        rhs: cand.rhs.clone(),
        index: Operand::Copy(Place::Local(idx)),
        splat_rhs: cand.splat_rhs.clone(),
        ptr_addr: cand.ptr_addr,
    };

    let orig_store = scalar_store(&cand, idx);
    let scalar_incs: Vec<Statement> = increment_stmts(func, latch, idx, 1, 1, esize, &cand);

    func.block_mut(cand.body_blk).stmts[cand.stmt_idx] = simd_stmt;
    rewrite_increments(func, latch, idx, l, esize * l, &cand);

    let add_tmp = new_temp(func, interner.int());
    let cmp_simd = new_temp(func, interner.bool());
    let cmp_tail = new_temp(func, interner.bool());

    let rem_header = BlockId(func.blocks.len() as u32);
    let scalar_body = BlockId(func.blocks.len() as u32 + 1);

    func.block_mut(header).stmts.push(Statement::Assign(
        Place::Local(add_tmp),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(idx)),
            Operand::Const(Const::Int(l)),
        ),
    ));
    func.block_mut(header).stmts.push(Statement::Assign(
        Place::Local(cmp_simd),
        Rvalue::Binary(
            BinOp::Le,
            Operand::Copy(Place::Local(add_tmp)),
            n_bound.clone(),
        ),
    ));
    func.block_mut(header).terminator = Terminator::If {
        cond: Operand::Copy(Place::Local(cmp_simd)),
        then_blk: orig_then,
        else_blk: rem_header,
    };

    let mut scalar_stmts = vec![orig_store];
    scalar_stmts.extend(scalar_incs);
    func.blocks.push(crate::BasicBlock {
        stmts: vec![Statement::Assign(
            Place::Local(cmp_tail),
            Rvalue::Binary(BinOp::Lt, Operand::Copy(Place::Local(idx)), n_bound),
        )],
        terminator: Terminator::If {
            cond: Operand::Copy(Place::Local(cmp_tail)),
            then_blk: scalar_body,
            else_blk: orig_else,
        },
    });
    func.blocks.push(crate::BasicBlock {
        stmts: scalar_stmts,
        terminator: Terminator::Goto(rem_header),
    });
    true
}

fn step_local(func: &MirFunction, latch: BlockId) -> Option<Local> {
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
                return Some(*d);
            }
        }
    }
    None
}

fn header_bound(func: &MirFunction, header: BlockId, idx: Local) -> Option<Operand> {
    let Terminator::If { cond, .. } = &func.block(header).terminator else {
        return None;
    };
    let Operand::Copy(Place::Local(c)) = cond else {
        return None;
    };
    for stmt in func.block(header).stmts.iter().rev() {
        if let Statement::Assign(Place::Local(d), Rvalue::Binary(BinOp::Lt, lhs, rhs)) = stmt {
            if d.0 == c.0 && index_is(lhs, idx) {
                return Some(rhs.clone());
            }
        }
    }
    None
}

fn find_candidate(
    func: &MirFunction,
    interner: &TypeInterner,
    body: &BTreeSet<BlockId>,
    header: BlockId,
    latch: BlockId,
    idx: Local,
) -> Option<Candidate> {
    if let Some(c) = match_store_block(func, interner, latch, idx, true) {
        return Some(c);
    }
    let inner: Vec<BlockId> = body
        .iter()
        .copied()
        .filter(|b| *b != latch && *b != header)
        .collect();
    if inner.len() != 1 {
        return None;
    }
    match_store_block(func, interner, inner[0], idx, false)
}

fn match_store_block(
    func: &MirFunction,
    interner: &TypeInterner,
    blk: BlockId,
    idx: Local,
    in_latch: bool,
) -> Option<Candidate> {
    let stmts = &func.block(blk).stmts;
    let mut found = None;
    for (i, stmt) in stmts.iter().enumerate() {
        if matches!(
            stmt,
            Statement::SourceLine(_) | Statement::Nop | Statement::DebugLine(_)
        ) {
            continue;
        }
        if let Some(c) = match_store(func, interner, stmts, i, idx) {
            if found.is_some() {
                return None;
            }
            found = Some((i, c));
        } else if !in_latch && !is_addr_temp(stmt) {
            return None;
        }
    }
    found.map(|(stmt_idx, mut c)| {
        c.stmt_idx = stmt_idx;
        c.in_latch = in_latch;
        c.body_blk = blk;
        c
    })
}

fn is_addr_temp(stmt: &Statement) -> bool {
    matches!(
        stmt,
        Statement::Assign(
            _,
            Rvalue::Binary(BinOp::Add | BinOp::Sub | BinOp::Mul, _, _)
        ) | Statement::Assign(
            Place::Local(_),
            Rvalue::Use(Operand::Copy(Place::Index { .. } | Place::Deref { .. }))
        ) | Statement::SourceLine(_)
            | Statement::Nop
            | Statement::DebugLine(_)
    )
}

fn match_store(
    func: &MirFunction,
    interner: &TypeInterner,
    stmts: &[Statement],
    stmt_i: usize,
    idx: Local,
) -> Option<Candidate> {
    match &stmts[stmt_i] {
        Statement::Assign(
            Place::Index {
                base: dest,
                index,
                unchecked: true,
            },
            rv,
        ) if index_is(index, idx) => {
            let (op, lhs, rhs) = binop_from_rvalue(stmts, rv)?;
            let elem = array_elem(interner, func.local_ty(*dest))?;
            let lane = SimdLane::from_elem(interner, elem)?;
            let (lhs_op, rhs_op, splat) = binop_operands(stmts, &lhs, &rhs, idx)?;
            Some(Candidate {
                stmt_idx: 0,
                in_latch: false,
                body_blk: BlockId(0),
                op,
                lane,
                dest: *dest,
                lhs: lhs_op,
                rhs: rhs_op,
                splat_rhs: splat,
                ptr_addr: false,
                dest_ptr: None,
                lhs_ptr: None,
                rhs_ptr: None,
                elem_ty: elem,
            })
        }
        Statement::Assign(Place::Deref { ptr, elem_ty }, rv) => {
            let (op, lhs, rhs) = binop_from_rvalue(stmts, rv)?;
            let lane = SimdLane::from_elem(interner, *elem_ty)?;
            let lhs_ptr = resolve_deref_ptr(stmts, &lhs)?;
            let (rhs_ptr, splat) = match resolve_deref_ptr(stmts, &rhs) {
                Some(p) => (Some(p), None),
                None if is_splat(&rhs) => (None, Some(rhs.clone())),
                None => return None,
            };
            Some(Candidate {
                stmt_idx: 0,
                in_latch: false,
                body_blk: BlockId(0),
                op,
                lane,
                dest: *ptr,
                lhs: Operand::Copy(Place::Local(lhs_ptr)),
                rhs: rhs_ptr
                    .map(|p| Operand::Copy(Place::Local(p)))
                    .unwrap_or_else(|| rhs.clone()),
                splat_rhs: splat,
                ptr_addr: true,
                dest_ptr: Some(*ptr),
                lhs_ptr: Some(lhs_ptr),
                rhs_ptr,
                elem_ty: *elem_ty,
            })
        }
        _ => None,
    }
}

fn binop_from_rvalue(stmts: &[Statement], rv: &Rvalue) -> Option<(BinOp, Operand, Operand)> {
    match rv {
        Rvalue::Binary(op, lhs, rhs) if matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul) => {
            Some((*op, lhs.clone(), rhs.clone()))
        }
        Rvalue::Use(Operand::Copy(Place::Local(tmp))) => {
            for stmt in stmts {
                if let Statement::Assign(Place::Local(d), Rvalue::Binary(op, lhs, rhs)) = stmt {
                    if d.0 == tmp.0 && matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul) {
                        return Some((*op, lhs.clone(), rhs.clone()));
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn binop_operands(
    stmts: &[Statement],
    lhs: &Operand,
    rhs: &Operand,
    idx: Local,
) -> Option<(Operand, Operand, Option<Operand>)> {
    let lhs_arr = resolve_index_arr(stmts, lhs, idx)?;
    if let Some(rhs_arr) = resolve_index_arr(stmts, rhs, idx) {
        return Some((
            Operand::Copy(Place::Local(lhs_arr)),
            Operand::Copy(Place::Local(rhs_arr)),
            None,
        ));
    }
    if is_splat(rhs) {
        return Some((
            Operand::Copy(Place::Local(lhs_arr)),
            rhs.clone(),
            Some(rhs.clone()),
        ));
    }
    if is_splat(lhs) {
        if let Some(rhs_arr) = resolve_index_arr(stmts, rhs, idx) {
            return Some((
                Operand::Copy(Place::Local(rhs_arr)),
                lhs.clone(),
                Some(lhs.clone()),
            ));
        }
    }
    None
}

fn resolve_index_arr(stmts: &[Statement], op: &Operand, idx: Local) -> Option<Local> {
    if let Some(base) = index_base(op, idx) {
        return Some(base);
    }
    let Operand::Copy(Place::Local(tmp)) = op else {
        return None;
    };
    for stmt in stmts {
        if let Statement::Assign(Place::Local(d), Rvalue::Use(src)) = stmt {
            if d.0 == tmp.0 {
                return index_base(src, idx);
            }
        }
    }
    None
}

fn resolve_deref_ptr(stmts: &[Statement], op: &Operand) -> Option<Local> {
    if let Some(p) = deref_ptr(op) {
        return Some(p);
    }
    let Operand::Copy(Place::Local(tmp)) = op else {
        return None;
    };
    for stmt in stmts {
        if let Statement::Assign(Place::Local(d), Rvalue::Use(src)) = stmt {
            if d.0 == tmp.0 {
                return deref_ptr(src);
            }
        }
    }
    None
}

fn is_splat(op: &Operand) -> bool {
    matches!(
        op,
        Operand::Const(_) | Operand::Copy(Place::Local(_) | Place::Global(_))
    )
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

fn deref_ptr(op: &Operand) -> Option<Local> {
    match op {
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

fn scalar_store(cand: &Candidate, idx: Local) -> Statement {
    let dest = if cand.ptr_addr {
        Place::Deref {
            ptr: cand.dest,
            elem_ty: cand.elem_ty,
        }
    } else {
        index_place(cand.dest, idx)
    };
    let lhs = if cand.ptr_addr {
        Operand::Copy(Place::Deref {
            ptr: cand.lhs_ptr.unwrap_or(cand.dest),
            elem_ty: cand.elem_ty,
        })
    } else {
        let base = operand_local(&cand.lhs).unwrap_or(cand.dest);
        Operand::Copy(index_place(base, idx))
    };
    let rhs = if let Some(s) = &cand.splat_rhs {
        s.clone()
    } else if cand.ptr_addr {
        Operand::Copy(Place::Deref {
            ptr: cand.rhs_ptr.unwrap_or(cand.dest),
            elem_ty: cand.elem_ty,
        })
    } else {
        let base = operand_local(&cand.rhs).unwrap_or(cand.dest);
        Operand::Copy(index_place(base, idx))
    };
    Statement::Assign(dest, Rvalue::Binary(cand.op, lhs, rhs))
}

fn index_place(base: Local, idx: Local) -> Place {
    Place::Index {
        base,
        index: Box::new(Operand::Copy(Place::Local(idx))),
        unchecked: true,
    }
}

fn operand_local(op: &Operand) -> Option<Local> {
    match op {
        Operand::Copy(Place::Local(l)) => Some(*l),
        _ => None,
    }
}

fn increment_stmts(
    func: &MirFunction,
    latch: BlockId,
    idx: Local,
    idx_step: i64,
    ptr_mul: i64,
    esize: i64,
    cand: &Candidate,
) -> Vec<Statement> {
    let mut out = Vec::new();
    out.push(Statement::Assign(
        Place::Local(idx),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(idx)),
            Operand::Const(Const::Int(idx_step)),
        ),
    ));
    if cand.ptr_addr {
        for p in [cand.dest_ptr, cand.lhs_ptr, cand.rhs_ptr]
            .iter()
            .copied()
            .flatten()
        {
            out.push(Statement::Assign(
                Place::Local(p),
                Rvalue::Binary(
                    BinOp::Add,
                    Operand::Copy(Place::Local(p)),
                    Operand::Const(Const::Int(esize * ptr_mul)),
                ),
            ));
        }
    }
    let _ = (func, latch);
    out
}

fn rewrite_increments(
    func: &mut MirFunction,
    latch: BlockId,
    idx: Local,
    idx_step: i64,
    ptr_step: i64,
    cand: &Candidate,
) {
    for stmt in &mut func.block_mut(latch).stmts {
        if let Statement::Assign(
            Place::Local(d),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(a)),
                Operand::Const(Const::Int(k)),
            ),
        ) = stmt
        {
            if d.0 == a.0 && d.0 == idx.0 && *k == 1 {
                *stmt = Statement::Assign(
                    Place::Local(idx),
                    Rvalue::Binary(
                        BinOp::Add,
                        Operand::Copy(Place::Local(idx)),
                        Operand::Const(Const::Int(idx_step)),
                    ),
                );
            } else if cand.ptr_addr && d.0 == a.0 {
                let is_ptr = cand.dest_ptr == Some(*d)
                    || cand.lhs_ptr == Some(*d)
                    || cand.rhs_ptr == Some(*d);
                if is_ptr {
                    *stmt = Statement::Assign(
                        Place::Local(*d),
                        Rvalue::Binary(
                            BinOp::Add,
                            Operand::Copy(Place::Local(*d)),
                            Operand::Const(Const::Int(ptr_step)),
                        ),
                    );
                }
            }
        }
    }
}

fn new_temp(func: &mut MirFunction, ty: dream_types::TypeId) -> Local {
    let id = Local(func.locals.len() as u32);
    func.locals.push(LocalDecl {
        ty,
        name: None,
        is_ref: false,
        is_take: false,
        is_cursor: false,
        manual_drop: false,
    });
    id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use crate::{Operand, Place, Rvalue, Terminator};

    fn counted_add(
        interner: &TypeInterner,
        arr_ty: dream_types::TypeId,
        unchecked: bool,
    ) -> MirFunction {
        let mut b = FunctionBuilder::new("f", interner.int());
        let dest = b.new_param(arr_ty, Some("c".into()));
        let lhs = b.new_param(arr_ty, Some("a".into()));
        let rhs = b.new_param(arr_ty, Some("b".into()));
        let n = b.new_param(interner.int(), Some("n".into()));
        let idx = b.new_temp(interner.int());
        let cmp = b.new_temp(interner.bool());
        b.assign(
            Place::Local(idx),
            Rvalue::Use(Operand::Const(Const::Int(0))),
        );
        let header = b.new_block();
        let body = b.new_block();
        let after = b.new_block();
        b.terminate(Terminator::Goto(header));
        b.switch_to(header);
        b.assign(
            Place::Local(cmp),
            Rvalue::Binary(
                BinOp::Lt,
                Operand::Copy(Place::Local(idx)),
                Operand::Copy(Place::Local(n)),
            ),
        );
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(cmp)),
            then_blk: body,
            else_blk: after,
        });
        b.switch_to(body);
        let ix = |base: Local| Place::Index {
            base,
            index: Box::new(Operand::Copy(Place::Local(idx))),
            unchecked,
        };
        b.assign(
            ix(dest),
            Rvalue::Binary(BinOp::Add, Operand::Copy(ix(lhs)), Operand::Copy(ix(rhs))),
        );
        b.assign(
            Place::Local(idx),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(idx)),
                Operand::Const(Const::Int(1)),
            ),
        );
        b.terminate(Terminator::Goto(header));
        b.switch_to(after);
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(idx)))));
        b.finish()
    }

    fn has_simd(func: &MirFunction) -> bool {
        func.blocks.iter().any(|b| {
            b.stmts
                .iter()
                .any(|s| matches!(s, Statement::SimdV128 { .. }))
        })
    }

    fn simd_step(func: &MirFunction) -> Option<i64> {
        for b in &func.blocks {
            for s in &b.stmts {
                if let Statement::Assign(
                    _,
                    Rvalue::Binary(BinOp::Add, _, Operand::Const(Const::Int(k))),
                ) = s
                {
                    if *k == 4 {
                        return Some(*k);
                    }
                }
            }
        }
        None
    }

    #[test]
    fn vectorizes_unchecked_float_add() {
        let mut i = TypeInterner::new();
        let arr = i.array(i.float());
        let mut func = counted_add(&i, arr, true);
        assert!(Autovec.run(&mut func, &i));
        assert!(has_simd(&func));
        assert_eq!(simd_step(&func), Some(4));
        let rem = func.blocks.iter().any(|b| {
            b.stmts.iter().any(|s| {
                matches!(
                    s,
                    Statement::Assign(Place::Index { .. }, Rvalue::Binary(BinOp::Add, _, _))
                )
            })
        });
        assert!(rem, "scalar remainder store must remain");
    }

    #[test]
    fn vectorizes_unchecked_int_add() {
        let mut i = TypeInterner::new();
        let arr = i.array(i.int());
        let mut func = counted_add(&i, arr, true);
        assert!(Autovec.run(&mut func, &i));
        assert!(has_simd(&func));
        match func
            .blocks
            .iter()
            .flat_map(|b| &b.stmts)
            .find(|s| matches!(s, Statement::SimdV128 { .. }))
        {
            Some(Statement::SimdV128 { lane, .. }) => assert_eq!(*lane, SimdLane::I32),
            other => panic!("expected SimdV128, got {:?}", other),
        }
    }

    #[test]
    fn rejects_checked_index() {
        let mut i = TypeInterner::new();
        let arr = i.array(i.float());
        let mut func = counted_add(&i, arr, false);
        assert!(!Autovec.run(&mut func, &i));
        assert!(!has_simd(&func));
    }

    #[test]
    fn splat_rhs_float() {
        let mut i = TypeInterner::new();
        let arr_ty = i.array(i.float());
        let mut b = FunctionBuilder::new("f", i.int());
        let dest = b.new_param(arr_ty, Some("c".into()));
        let lhs = b.new_param(arr_ty, Some("a".into()));
        let n = b.new_param(i.int(), Some("n".into()));
        let idx = b.new_temp(i.int());
        let cmp = b.new_temp(i.bool());
        b.assign(
            Place::Local(idx),
            Rvalue::Use(Operand::Const(Const::Int(0))),
        );
        let header = b.new_block();
        let body = b.new_block();
        let after = b.new_block();
        b.terminate(Terminator::Goto(header));
        b.switch_to(header);
        b.assign(
            Place::Local(cmp),
            Rvalue::Binary(
                BinOp::Lt,
                Operand::Copy(Place::Local(idx)),
                Operand::Copy(Place::Local(n)),
            ),
        );
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(cmp)),
            then_blk: body,
            else_blk: after,
        });
        b.switch_to(body);
        let ix = |base: Local| Place::Index {
            base,
            index: Box::new(Operand::Copy(Place::Local(idx))),
            unchecked: true,
        };
        b.assign(
            ix(dest),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(ix(lhs)),
                Operand::Const(Const::F32(1.0)),
            ),
        );
        b.assign(
            Place::Local(idx),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(idx)),
                Operand::Const(Const::Int(1)),
            ),
        );
        b.terminate(Terminator::Goto(header));
        b.switch_to(after);
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(idx)))));
        let mut func = b.finish();
        assert!(Autovec.run(&mut func, &i));
        let splat = func.blocks.iter().flat_map(|b| &b.stmts).any(|s| {
            matches!(
                s,
                Statement::SimdV128 {
                    splat_rhs: Some(_),
                    ..
                }
            )
        });
        assert!(splat);
    }

    #[test]
    fn vectorizes_split_load_add_store() {
        let mut i = TypeInterner::new();
        let arr_ty = i.array(i.float());
        let mut b = FunctionBuilder::new("f", i.int());
        let dest = b.new_param(arr_ty, Some("c".into()));
        let lhs = b.new_param(arr_ty, Some("a".into()));
        let rhs = b.new_param(arr_ty, Some("b".into()));
        let n = b.new_param(i.int(), Some("n".into()));
        let idx = b.new_temp(i.int());
        let cmp = b.new_temp(i.bool());
        let t1 = b.new_temp(i.float());
        let t2 = b.new_temp(i.float());
        let t3 = b.new_temp(i.float());
        b.assign(
            Place::Local(idx),
            Rvalue::Use(Operand::Const(Const::Int(0))),
        );
        let header = b.new_block();
        let body = b.new_block();
        let after = b.new_block();
        b.terminate(Terminator::Goto(header));
        b.switch_to(header);
        b.assign(
            Place::Local(cmp),
            Rvalue::Binary(
                BinOp::Lt,
                Operand::Copy(Place::Local(idx)),
                Operand::Copy(Place::Local(n)),
            ),
        );
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(cmp)),
            then_blk: body,
            else_blk: after,
        });
        b.switch_to(body);
        let ix = |base: Local| Place::Index {
            base,
            index: Box::new(Operand::Copy(Place::Local(idx))),
            unchecked: true,
        };
        b.assign(Place::Local(t1), Rvalue::Use(Operand::Copy(ix(lhs))));
        b.assign(Place::Local(t2), Rvalue::Use(Operand::Copy(ix(rhs))));
        b.assign(
            Place::Local(t3),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(t1)),
                Operand::Copy(Place::Local(t2)),
            ),
        );
        b.assign(ix(dest), Rvalue::Use(Operand::Copy(Place::Local(t3))));
        b.assign(
            Place::Local(idx),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(idx)),
                Operand::Const(Const::Int(1)),
            ),
        );
        b.terminate(Terminator::Goto(header));
        b.switch_to(after);
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(idx)))));
        let mut func = b.finish();
        assert!(Autovec.run(&mut func, &i));
        assert!(has_simd(&func));
    }
}
