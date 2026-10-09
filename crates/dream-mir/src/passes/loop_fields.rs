//! Cache unchanged scalar class fields in a guarded loop preheader.
//!
//! Concrete class field slots use the same `(TypeId, field)` identity as RC mod-ref.
//! Aliases may name the same object, but writes to another slot cannot change the
//! cached field. Calls, lifetime changes, raw stores and shared objects invalidate
//! this proof. The entry test and first iteration are peeled so zero-trip loops perform no
//! extra reads and the initial loads keep their original ordering.

use crate::{BlockId, Local, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefId, TyKind, TypeId, TypeInterner};
use std::collections::BTreeSet;

pub(super) fn run(mir: &mut Mir, interner: &TypeInterner) {
    let math: BTreeSet<_> = mir
        .imports
        .iter()
        .filter(|i| super::rc::modref::pure_math_import(i, interner))
        .map(|i| i.def)
        .collect();
    for f in &mut mir.functions {
        let mut analyses = super::FunctionAnalyses::default();
        while hoist(f, interner, &math, &mut analyses) {
            analyses.invalidate();
        }
    }
}

fn hoist(
    f: &mut MirFunction,
    i: &TypeInterner,
    math: &BTreeSet<DefId>,
    analyses: &mut super::FunctionAnalyses,
) -> bool {
    if f.is_async {
        return false;
    }
    let loops = analyses.natural_loops(f);
    for l in loops.iter() {
        if l.body.len() != 2 {
            continue;
        }
        let h = f.block(l.header);
        let Terminator::If {
            then_blk, else_blk, ..
        } = h.terminator
        else {
            continue;
        };
        if !l.body.contains(&then_blk)
            || l.body.contains(&else_blk)
            || !matches!(f.block(then_blk).terminator, Terminator::Goto(b) if b == l.header)
            || !h.stmts.iter().all(header_stmt)
        {
            continue;
        }
        let body = f.block(then_blk);
        let mut writes = BTreeSet::new();
        let mut defs = BTreeSet::new();
        let mut quiet = true;
        for s in h.stmts.iter().chain(&body.stmts) {
            match s {
                Statement::Assign(Place::Local(d), rv) => {
                    defs.insert(*d);
                    quiet &= !f.locals[d.0 as usize].is_ref && quiet_rvalue(rv, f, i, math);
                }
                Statement::Assign(Place::Field { base, field }, rv) => {
                    let ty = f.local_ty(*base);
                    quiet &= class_type(ty, i)
                        && !f.locals[base.0 as usize].is_ref
                        && quiet_rvalue(rv, f, i, math);
                    writes.insert((ty, *field));
                }
                Statement::Nop | Statement::DebugLine(_) | Statement::SourceLine(_) => {}
                _ => quiet = false,
            }
        }
        if !quiet {
            continue;
        }
        let mut candidates = Vec::new();
        for (pos, s) in body.stmts.iter().enumerate() {
            let Statement::Assign(
                Place::Local(d),
                Rvalue::Use(Operand::Copy(Place::Field { base, field })),
            ) = s
            else {
                continue;
            };
            let ty = f.local_ty(*base);
            if defs.contains(base)
                || !class_type(ty, i)
                || f.locals[base.0 as usize].is_ref
                || !matches!(i.kind(f.local_ty(*d)), TyKind::Prim(p) if *p != dream_types::PrimTy::String)
                || writes.contains(&(ty, *field))
                || !private_result(f, *d, then_blk, pos)
            {
                continue;
            }
            candidates.push(pos);
        }
        if candidates.is_empty() {
            continue;
        }
        let incoming: Vec<_> = analyses.predecessors(f)[l.header.0 as usize]
            .iter()
            .copied()
            .filter(|p| !l.body.contains(p))
            .collect();
        if incoming.is_empty() && f.entry != l.header {
            continue;
        }
        let guard = BlockId(f.blocks.len() as u32);
        let preheader = BlockId(guard.0 + 1);
        let mut test = h.clone();
        if let Terminator::If { then_blk, .. } = &mut test.terminator {
            *then_blk = preheader;
        }
        let first_iteration = body.clone();
        for pos in candidates {
            f.block_mut(then_blk).stmts[pos] = Statement::Nop;
        }
        f.blocks.push(test);
        f.blocks.push(first_iteration);
        for pred in incoming {
            redirect(&mut f.block_mut(pred).terminator, l.header, guard);
        }
        if f.entry == l.header {
            f.entry = guard;
        }
        return true;
    }
    false
}

fn class_type(ty: TypeId, i: &TypeInterner) -> bool {
    matches!(i.kind(ty), TyKind::Struct(..)) && !i.is_value_type(ty) && !i.is_shared_type(ty)
}

fn header_stmt(s: &Statement) -> bool {
    match s {
        Statement::Assign(Place::Local(_), rv) => match rv {
            Rvalue::Use(a) | Rvalue::Unary(_, a) => scalar_operand(a),
            Rvalue::Binary(op, a, b) => {
                !matches!(op, crate::BinOp::Div | crate::BinOp::Rem)
                    && scalar_operand(a)
                    && scalar_operand(b)
            }
            _ => false,
        },
        Statement::Nop | Statement::DebugLine(_) | Statement::SourceLine(_) => true,
        _ => false,
    }
}

fn scalar_operand(o: &Operand) -> bool {
    matches!(o, Operand::Const(_) | Operand::Copy(Place::Local(_)))
}

fn quiet_operand(o: &Operand, f: &MirFunction, i: &TypeInterner) -> bool {
    match o {
        Operand::Const(_) => true,
        Operand::Copy(Place::Local(l)) => !f.locals[l.0 as usize].is_ref,
        Operand::Copy(Place::Field { base, .. }) => {
            class_type(f.local_ty(*base), i) && !f.locals[base.0 as usize].is_ref
        }
        Operand::Copy(Place::Index {
            base,
            index,
            unchecked: true,
        }) => !f.locals[base.0 as usize].is_ref && scalar_operand(index),
        _ => false,
    }
}

fn quiet_rvalue(rv: &Rvalue, f: &MirFunction, i: &TypeInterner, math: &BTreeSet<DefId>) -> bool {
    match rv {
        Rvalue::Use(a) | Rvalue::Unary(_, a) | Rvalue::Cast(a, _, _) | Rvalue::ArrayLen(a) => {
            quiet_operand(a, f, i)
        }
        Rvalue::Binary(_, a, b) => quiet_operand(a, f, i) && quiet_operand(b, f, i),
        Rvalue::Call { callee, args } => {
            math.contains(&callee.def) && args.iter().all(scalar_operand)
        }
        _ => false,
    }
}

fn private_result(f: &MirFunction, d: Local, body: BlockId, pos: usize) -> bool {
    let mut count = 0;
    for (bi, b) in f.blocks.iter().enumerate() {
        for (si, s) in b.stmts.iter().enumerate() {
            if matches!(s, Statement::Assign(Place::Local(l), _) if *l == d) {
                count += 1;
            }
            let mut used = false;
            super::licm::stmt_reads(s, &mut |l| used |= l == d);
            if used && (bi != body.0 as usize || si <= pos) {
                return false;
            }
        }
        let mut used = false;
        super::licm::terminator_reads(&b.terminator, &mut |l| used |= l == d);
        if used && bi != body.0 as usize {
            return false;
        }
    }
    count == 1
}

fn redirect(t: &mut Terminator, from: BlockId, to: BlockId) {
    let fix = |b: &mut BlockId| {
        if *b == from {
            *b = to;
        }
    };
    match t {
        Terminator::Goto(b) => fix(b),
        Terminator::If {
            then_blk, else_blk, ..
        } => {
            fix(then_blk);
            fix(else_blk);
        }
        Terminator::Switch {
            targets, default, ..
        } => {
            for (_, b) in targets {
                fix(b);
            }
            fix(default);
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "loop_fields_tests.rs"]
mod tests;
