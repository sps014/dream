//! Array bounds-check elimination. Marks [`Place::Index`] `unchecked` when a dominating
//! `idx < len` branch plus a non-negative `idx` prove the WASM `ge_u` check cannot fire.

use super::cfg::DomTree;
use super::MirPass;
use crate::{BinOp, Const, Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;
use std::collections::HashSet;

pub struct Abc;

impl MirPass for Abc {
    fn name(&self) -> &'static str {
        "abc"
    }

    fn run(&self, func: &mut MirFunction, _interner: &TypeInterner) -> bool {
        let nonneg = nonnegative_locals(func);
        let facts = range_facts(func, &nonneg);
        if facts.is_empty() {
            return false;
        }
        let mut changed = false;
        for block in &mut func.blocks {
            for stmt in &mut block.stmts {
                changed |= mark_stmt(stmt, &facts);
            }
            changed |= mark_terminator(&mut block.terminator, &facts);
        }
        changed
    }
}

/// `(idx, arr)` pairs that are in-range in a given block (block index → set).
type Facts = Vec<HashSet<(u32, u32)>>;

fn range_facts(func: &MirFunction, nonneg: &HashSet<u32>) -> Facts {
    let n = func.blocks.len();
    let mut facts: Facts = vec![HashSet::new(); n];
    let dom = DomTree::new(func);
    let len_of = array_len_locals(func);

    for (bi, block) in func.blocks.iter().enumerate() {
        let Terminator::If {
            cond: Operand::Copy(Place::Local(cmp)),
            then_blk,
            ..
        } = &block.terminator
        else {
            continue;
        };
        let Some((idx, len)) = lt_operands(block, *cmp) else {
            continue;
        };
        if !nonneg.contains(&idx.0) {
            continue;
        }
        let Some(&arr) = len_of.get(&len.0) else {
            continue;
        };
        let header = crate::BlockId(bi as u32);
        for (ti, _) in func.blocks.iter().enumerate() {
            let t = crate::BlockId(ti as u32);
            if (t == *then_blk || dom.dominates(*then_blk, t))
                && !redefines_between(func, &dom, header, t, &[idx, len, Local(arr)])
            {
                facts[ti].insert((idx.0, arr));
            }
        }
    }
    facts
}

fn mark_stmt(stmt: &mut Statement, facts: &Facts) -> bool {
    let mut changed = false;
    match stmt {
        Statement::Assign(place, rv) => {
            changed |= mark_place(place, facts, 0);
            changed |= mark_rvalue(rv, facts);
        }
        Statement::Call { args, .. }
        | Statement::IndirectCall { args, .. }
        | Statement::InterfaceCall { args, .. } => {
            for a in args {
                changed |= mark_operand(a, facts);
            }
        }
        _ => {}
    }
    changed
}

fn mark_terminator(t: &mut Terminator, facts: &Facts) -> bool {
    match t {
        Terminator::If { cond, .. } => mark_operand(cond, facts),
        Terminator::Return(Some(o)) | Terminator::AsyncComplete(Some(o)) => mark_operand(o, facts),
        Terminator::Switch { value, .. } => mark_operand(value, facts),
        Terminator::Await { future, .. } => mark_operand(future, facts),
        Terminator::TailCall { args, .. } => {
            let mut c = false;
            for a in args {
                c |= mark_operand(a, facts);
            }
            c
        }
        _ => false,
    }
}

fn mark_rvalue(rv: &mut Rvalue, facts: &Facts) -> bool {
    match rv {
        Rvalue::Use(o) | Rvalue::Unary(_, o) | Rvalue::ArrayLen(o) => mark_operand(o, facts),
        Rvalue::Binary(_, a, b) => mark_operand(a, facts) | mark_operand(b, facts),
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => mark_operand(cond, facts) | mark_operand(then_val, facts) | mark_operand(else_val, facts),
        Rvalue::Call { args, .. } | Rvalue::New { args, .. } => {
            let mut c = false;
            for a in args {
                c |= mark_operand(a, facts);
            }
            c
        }
        Rvalue::InterfaceCall { receiver, args, .. } => {
            let mut c = mark_operand(receiver, facts);
            for a in args {
                c |= mark_operand(a, facts);
            }
            c
        }
        _ => false,
    }
}

fn mark_operand(op: &mut Operand, facts: &Facts) -> bool {
    match op {
        Operand::Copy(p) => mark_place(p, facts, 0),
        Operand::Const(_) => false,
    }
}

fn mark_place(place: &mut Place, facts: &Facts, block_hint: usize) -> bool {
    match place {
        Place::Index {
            base,
            index,
            unchecked,
        } if !*unchecked => {
            if let Operand::Copy(Place::Local(idx)) = index.as_ref() {
                if facts
                    .get(block_hint)
                    .map(|s| s.contains(&(idx.0, base.0)))
                    .unwrap_or(false)
                    || facts.iter().any(|s| s.contains(&(idx.0, base.0)))
                {
                    *unchecked = true;
                    return true;
                }
            }
            false
        }
        Place::Index { index, .. } => mark_operand(index, facts),
        _ => false,
    }
}

fn lt_operands(block: &crate::BasicBlock, cmp: Local) -> Option<(Local, Local)> {
    for stmt in block.stmts.iter().rev() {
        if let Statement::Assign(Place::Local(d), Rvalue::Binary(BinOp::Lt, a, b)) = stmt {
            if *d == cmp {
                let ia = as_local(a)?;
                let ib = as_local(b)?;
                return Some((ia, ib));
            }
        }
    }
    None
}

fn as_local(op: &Operand) -> Option<Local> {
    match op {
        Operand::Copy(Place::Local(l)) => Some(*l),
        _ => None,
    }
}

fn array_len_locals(func: &MirFunction) -> std::collections::HashMap<u32, u32> {
    let mut m = std::collections::HashMap::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            if let Statement::Assign(
                Place::Local(d),
                Rvalue::ArrayLen(Operand::Copy(Place::Local(arr))),
            ) = stmt
            {
                m.insert(d.0, arr.0);
            }
        }
    }
    m
}

fn nonnegative_locals(func: &MirFunction) -> HashSet<u32> {
    let mut nonneg: HashSet<u32> = HashSet::new();
    let mut changed = true;
    while changed {
        changed = false;
        for block in &func.blocks {
            for stmt in &block.stmts {
                let Statement::Assign(Place::Local(d), rv) = stmt else {
                    continue;
                };
                let ok = match rv {
                    Rvalue::Use(Operand::Const(Const::Int(v))) if *v >= 0 => true,
                    Rvalue::Use(Operand::Copy(Place::Local(s))) => nonneg.contains(&s.0),
                    Rvalue::Binary(BinOp::Add, a, b) => {
                        (as_local(a).is_some_and(|l| nonneg.contains(&l.0))
                            || matches!(a, Operand::Const(Const::Int(v)) if *v >= 0))
                            && (as_local(b).is_some_and(|l| nonneg.contains(&l.0))
                                || matches!(b, Operand::Const(Const::Int(v)) if *v >= 0))
                    }
                    Rvalue::ArrayLen(_) | Rvalue::StrLen(_) | Rvalue::StrByteSize(_) => true,
                    _ => false,
                };
                if ok && nonneg.insert(d.0) {
                    changed = true;
                }
            }
        }
    }
    nonneg
}

fn redefines_between(
    func: &MirFunction,
    dom: &DomTree,
    from: crate::BlockId,
    to: crate::BlockId,
    locals: &[Local],
) -> bool {
    for (i, block) in func.blocks.iter().enumerate() {
        let b = crate::BlockId(i as u32);
        if b == from {
            continue;
        }
        if !(dom.dominates(from, b) && (b == to || dom.dominates(b, to) || dom.dominates(to, b))) {
            continue;
        }
        if b != to && !dom.dominates(b, to) && !dom.dominates(to, b) {
            continue;
        }
        // Only blocks on the way from `from`'s then-successor toward `to`.
        if !dom.dominates(from, b) {
            continue;
        }
        for stmt in &block.stmts {
            if let Statement::Assign(Place::Local(d), _) = stmt {
                if locals.iter().any(|l| l.0 == d.0) && b != to {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use crate::{Operand, Place, Rvalue, Terminator};

    #[test]
    fn foreach_shape_is_unchecked() {
        let mut i = TypeInterner::new();
        let arr_ty = i.array(i.int());
        let mut b = FunctionBuilder::new("f", i.int());
        let arr = b.new_param(arr_ty, Some("a".into()));
        let idx = b.new_temp(i.int());
        let len = b.new_temp(i.int());
        let cmp = b.new_temp(i.bool());
        let elem = b.new_temp(i.int());
        b.assign(Place::Local(idx), Rvalue::Use(Operand::Const(Const::Int(0))));
        b.assign(
            Place::Local(len),
            Rvalue::ArrayLen(Operand::Copy(Place::Local(arr))),
        );
        let cond = b.new_block();
        let body = b.new_block();
        let after = b.new_block();
        b.terminate(Terminator::Goto(cond));
        b.switch_to(cond);
        b.assign(
            Place::Local(cmp),
            Rvalue::Binary(
                BinOp::Lt,
                Operand::Copy(Place::Local(idx)),
                Operand::Copy(Place::Local(len)),
            ),
        );
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(cmp)),
            then_blk: body,
            else_blk: after,
        });
        b.switch_to(body);
        b.assign(
            Place::Local(elem),
            Rvalue::Use(Operand::Copy(Place::index(
                arr,
                Operand::Copy(Place::Local(idx)),
            ))),
        );
        b.assign(
            Place::Local(idx),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(idx)),
                Operand::Const(Const::Int(1)),
            ),
        );
        b.terminate(Terminator::Goto(cond));
        b.switch_to(after);
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(elem)))));
        let mut func = b.finish();
        assert!(Abc.run(&mut func, &i));
        match &func.blocks[body.0 as usize].stmts[0] {
            Statement::Assign(_, Rvalue::Use(Operand::Copy(Place::Index { unchecked, .. }))) => {
                assert!(*unchecked);
            }
            other => panic!("expected unchecked index, got {:?}", other),
        }
    }
}
