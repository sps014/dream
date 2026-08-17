//! Array and string bounds-check elimination. Marks [`Place::Index`] / [`Rvalue::CharAt`] /
//! [`Rvalue::ByteAt`] `unchecked` when a dominating `idx < len` branch plus a non-negative `idx`
//! prove the WASM `ge_u` check cannot fire.

use super::cfg::DomTree;
use super::MirPass;
use crate::{BinOp, Const, Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;
use std::collections::HashSet;

pub struct Abc;

#[derive(Clone, PartialEq, Eq, Hash)]
enum StrBase {
    Local(u32),
    Lit(String),
}

impl MirPass for Abc {
    fn name(&self) -> &'static str {
        "abc"
    }

    fn run(&self, func: &mut MirFunction, _interner: &TypeInterner) -> bool {
        let nonneg = nonnegative_locals(func);
        let (arr_facts, char_facts, byte_facts) = range_facts(func, &nonneg);
        if arr_facts.iter().all(|s| s.is_empty())
            && char_facts.iter().all(|s| s.is_empty())
            && byte_facts.iter().all(|s| s.is_empty())
        {
            return false;
        }
        let mut changed = false;
        for (bi, block) in func.blocks.iter_mut().enumerate() {
            for stmt in &mut block.stmts {
                changed |= mark_stmt(stmt, &arr_facts, &char_facts, &byte_facts, bi);
            }
            changed |= mark_terminator(&mut block.terminator, &arr_facts);
        }
        changed
    }
}

/// `(idx, array-local)` pairs that are in-range in a given block.
type ArrFacts = Vec<HashSet<(u32, u32)>>;
/// `(idx, string)` pairs (`Local` or interned literal) in range in a given block.
type StrFacts = Vec<HashSet<(u32, StrBase)>>;

fn range_facts(func: &MirFunction, nonneg: &HashSet<u32>) -> (ArrFacts, StrFacts, StrFacts) {
    let n = func.blocks.len();
    let mut arr_facts: ArrFacts = vec![HashSet::new(); n];
    let mut char_facts: StrFacts = vec![HashSet::new(); n];
    let mut byte_facts: StrFacts = vec![HashSet::new(); n];
    let dom = DomTree::new(func);
    let len_of = array_len_locals(func);
    let unit_of = string_len_locals(func, false);
    let bytes_of = string_len_locals(func, true);

    for (bi, block) in func.blocks.iter().enumerate() {
        let Terminator::If {
            cond: Operand::Copy(Place::Local(cmp)),
            then_blk,
            ..
        } = &block.terminator
        else {
            continue;
        };
        let Some((idx, bound)) = lt_bound(block, *cmp) else {
            continue;
        };
        if !nonneg.contains(&idx.0) {
            continue;
        }
        let arrs = arrays_bounded_by(func, &len_of, &bound);
        let unit_strs = strings_bounded_by(&unit_of, &bound);
        let byte_strs = strings_bounded_by(&bytes_of, &bound);
        if arrs.is_empty() && unit_strs.is_empty() && byte_strs.is_empty() {
            continue;
        }
        let header = crate::BlockId(bi as u32);
        for (ti, _) in func.blocks.iter().enumerate() {
            let t = crate::BlockId(ti as u32);
            if (t == *then_blk || dom.dominates(*then_blk, t))
                && !redefines_between(func, &dom, header, t, &[idx])
            {
                for &arr in &arrs {
                    if !redefines_between(func, &dom, header, t, &[Local(arr)]) {
                        arr_facts[ti].insert((idx.0, arr));
                    }
                }
                for s in &unit_strs {
                    if str_base_redefined(func, &dom, header, t, s) {
                        continue;
                    }
                    char_facts[ti].insert((idx.0, s.clone()));
                }
                for s in &byte_strs {
                    if str_base_redefined(func, &dom, header, t, s) {
                        continue;
                    }
                    byte_facts[ti].insert((idx.0, s.clone()));
                }
            }
        }
    }
    (arr_facts, char_facts, byte_facts)
}

fn str_base_redefined(
    func: &MirFunction,
    dom: &DomTree,
    from: crate::BlockId,
    to: crate::BlockId,
    base: &StrBase,
) -> bool {
    match base {
        StrBase::Local(l) => redefines_between(func, dom, from, to, &[Local(*l)]),
        StrBase::Lit(_) => false,
    }
}

fn str_base(op: &Operand) -> Option<StrBase> {
    match op {
        Operand::Copy(Place::Local(l)) => Some(StrBase::Local(l.0)),
        Operand::Const(Const::Str(s)) => Some(StrBase::Lit(s.clone())),
        _ => None,
    }
}

fn mark_stmt(
    stmt: &mut Statement,
    arr_facts: &ArrFacts,
    char_facts: &StrFacts,
    byte_facts: &StrFacts,
    bi: usize,
) -> bool {
    let mut changed = false;
    match stmt {
        Statement::Assign(place, rv) => {
            changed |= mark_place(place, arr_facts, bi);
            changed |= mark_rvalue(rv, arr_facts, char_facts, byte_facts, bi);
        }
        Statement::Call { args, .. }
        | Statement::IndirectCall { args, .. }
        | Statement::InterfaceCall { args, .. } => {
            for a in args {
                changed |= mark_operand(a, arr_facts, bi);
            }
        }
        _ => {}
    }
    changed
}

fn mark_terminator(t: &mut Terminator, facts: &ArrFacts) -> bool {
    match t {
        Terminator::If { cond, .. } => mark_operand(cond, facts, 0),
        Terminator::Return(Some(o)) | Terminator::AsyncComplete(Some(o)) => {
            mark_operand(o, facts, 0)
        }
        Terminator::Switch { value, .. } => mark_operand(value, facts, 0),
        Terminator::Await { future, .. } => mark_operand(future, facts, 0),
        Terminator::TailCall { args, .. } => {
            let mut c = false;
            for a in args {
                c |= mark_operand(a, facts, 0);
            }
            c
        }
        _ => false,
    }
}

fn in_arr_facts(facts: &ArrFacts, bi: usize, idx: u32, base: u32) -> bool {
    facts
        .get(bi)
        .map(|s| s.contains(&(idx, base)))
        .unwrap_or(false)
        || facts.iter().any(|s| s.contains(&(idx, base)))
}

fn in_str_facts(facts: &StrFacts, bi: usize, idx: u32, base: &StrBase) -> bool {
    let key = (idx, base.clone());
    facts.get(bi).map(|s| s.contains(&key)).unwrap_or(false)
        || facts.iter().any(|s| s.contains(&key))
}

fn mark_rvalue(
    rv: &mut Rvalue,
    arr_facts: &ArrFacts,
    char_facts: &StrFacts,
    byte_facts: &StrFacts,
    bi: usize,
) -> bool {
    match rv {
        Rvalue::Use(o) | Rvalue::Unary(_, o) | Rvalue::ArrayLen(o) => {
            mark_operand(o, arr_facts, bi)
        }
        Rvalue::Binary(_, a, b) => mark_operand(a, arr_facts, bi) | mark_operand(b, arr_facts, bi),
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => {
            mark_operand(cond, arr_facts, bi)
                | mark_operand(then_val, arr_facts, bi)
                | mark_operand(else_val, arr_facts, bi)
        }
        Rvalue::Call { args, .. } | Rvalue::New { args, .. } => {
            let mut c = false;
            for a in args {
                c |= mark_operand(a, arr_facts, bi);
            }
            c
        }
        Rvalue::InterfaceCall { receiver, args, .. } => {
            let mut c = mark_operand(receiver, arr_facts, bi);
            for a in args {
                c |= mark_operand(a, arr_facts, bi);
            }
            c
        }
        Rvalue::CharAt(s, i, unchecked) if !*unchecked => {
            if let Operand::Copy(Place::Local(idx)) = i {
                if let Some(base) = str_base(s) {
                    if in_str_facts(char_facts, bi, idx.0, &base) {
                        *unchecked = true;
                        return true;
                    }
                }
            }
            false
        }
        Rvalue::ByteAt(s, i, unchecked) if !*unchecked => {
            if let Operand::Copy(Place::Local(idx)) = i {
                if let Some(base) = str_base(s) {
                    if in_str_facts(byte_facts, bi, idx.0, &base) {
                        *unchecked = true;
                        return true;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

fn mark_operand(op: &mut Operand, facts: &ArrFacts, block_hint: usize) -> bool {
    match op {
        Operand::Copy(p) => mark_place(p, facts, block_hint),
        Operand::Const(_) => false,
    }
}

fn mark_place(place: &mut Place, facts: &ArrFacts, block_hint: usize) -> bool {
    match place {
        Place::Index {
            base,
            index,
            unchecked,
        } if !*unchecked => {
            if let Operand::Copy(Place::Local(idx)) = index.as_ref() {
                if in_arr_facts(facts, block_hint, idx.0, base.0) {
                    *unchecked = true;
                    return true;
                }
            }
            false
        }
        Place::Index { index, .. } => mark_operand(index, facts, block_hint),
        _ => false,
    }
}

fn lt_bound(block: &crate::BasicBlock, cmp: Local) -> Option<(Local, Operand)> {
    for stmt in block.stmts.iter().rev() {
        if let Statement::Assign(Place::Local(d), Rvalue::Binary(BinOp::Lt, a, b)) = stmt {
            if *d == cmp {
                let ia = as_local(a)?;
                return Some((ia, b.clone()));
            }
        }
    }
    None
}

fn arrays_bounded_by(
    func: &MirFunction,
    len_of: &std::collections::HashMap<u32, Vec<u32>>,
    bound: &Operand,
) -> Vec<u32> {
    match bound {
        Operand::Copy(Place::Local(n)) => {
            let mut arrs = len_of.get(&n.0).cloned().unwrap_or_default();
            if let Some(k) = local_const_int(func, *n) {
                arrs.extend(arrays_alloced_with_const(func, k));
            }
            arrs
        }
        Operand::Const(Const::Int(k)) => arrays_alloced_with_const(func, *k),
        _ => Vec::new(),
    }
}

fn local_const_int(func: &MirFunction, local: Local) -> Option<i64> {
    for block in &func.blocks {
        for stmt in &block.stmts {
            if let Statement::Assign(Place::Local(d), Rvalue::Use(Operand::Const(Const::Int(k)))) =
                stmt
            {
                if *d == local {
                    return Some(*k);
                }
            }
        }
    }
    None
}

fn arrays_alloced_with_const(func: &MirFunction, k: i64) -> Vec<u32> {
    let mut arrs = Vec::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            if let Statement::Assign(Place::Local(arr), Rvalue::ArrayNew { len, .. }) = stmt {
                match len {
                    Operand::Const(Const::Int(v)) if *v == k => arrs.push(arr.0),
                    Operand::Copy(Place::Local(n)) if local_const_int(func, *n) == Some(k) => {
                        arrs.push(arr.0);
                    }
                    _ => {}
                }
            }
        }
    }
    arrs
}

fn as_local(op: &Operand) -> Option<Local> {
    match op {
        Operand::Copy(Place::Local(l)) => Some(*l),
        _ => None,
    }
}

fn array_len_locals(func: &MirFunction) -> std::collections::HashMap<u32, Vec<u32>> {
    let mut m: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            match stmt {
                Statement::Assign(
                    Place::Local(d),
                    Rvalue::ArrayLen(Operand::Copy(Place::Local(arr))),
                ) => {
                    m.entry(d.0).or_default().push(arr.0);
                }
                Statement::Assign(
                    Place::Local(arr),
                    Rvalue::ArrayNew {
                        len: Operand::Copy(Place::Local(n)),
                        ..
                    },
                ) => {
                    m.entry(n.0).or_default().push(arr.0);
                }
                _ => {}
            }
        }
    }
    m
}

fn string_len_locals(
    func: &MirFunction,
    byte_size: bool,
) -> std::collections::HashMap<u32, Vec<StrBase>> {
    let mut m: std::collections::HashMap<u32, Vec<StrBase>> = std::collections::HashMap::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            match stmt {
                Statement::Assign(Place::Local(d), Rvalue::StrLen(op)) if !byte_size => {
                    if let Some(b) = str_base(op) {
                        m.entry(d.0).or_default().push(b);
                    }
                }
                Statement::Assign(Place::Local(d), Rvalue::StrByteSize(op)) if byte_size => {
                    if let Some(b) = str_base(op) {
                        m.entry(d.0).or_default().push(b);
                    }
                }
                _ => {}
            }
        }
    }
    m
}

fn strings_bounded_by(
    len_of: &std::collections::HashMap<u32, Vec<StrBase>>,
    bound: &Operand,
) -> Vec<StrBase> {
    match bound {
        Operand::Copy(Place::Local(n)) => len_of.get(&n.0).cloned().unwrap_or_default(),
        _ => Vec::new(),
    }
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
        b.assign(
            Place::Local(idx),
            Rvalue::Use(Operand::Const(Const::Int(0))),
        );
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

    #[test]
    fn alloc_len_bound_is_unchecked() {
        let mut i = TypeInterner::new();
        let arr_ty = i.array(i.float());
        let mut b = FunctionBuilder::new("f", i.int());
        let n = b.new_param(i.int(), Some("n".into()));
        let arr = b.new_temp(arr_ty);
        let idx = b.new_temp(i.int());
        let cmp = b.new_temp(i.bool());
        let elem = b.new_temp(i.float());
        b.assign(
            Place::Local(arr),
            Rvalue::ArrayNew {
                elem_ty: i.float(),
                len: Operand::Copy(Place::Local(n)),
            },
        );
        b.assign(
            Place::Local(idx),
            Rvalue::Use(Operand::Const(Const::Int(0))),
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
                Operand::Copy(Place::Local(n)),
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
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(n)))));
        let mut func = b.finish();
        assert!(Abc.run(&mut func, &i));
        match &func.blocks[body.0 as usize].stmts[0] {
            Statement::Assign(_, Rvalue::Use(Operand::Copy(Place::Index { unchecked, .. }))) => {
                assert!(*unchecked);
            }
            other => panic!("expected unchecked index, got {:?}", other),
        }
    }

    #[test]
    fn char_at_scan_shape_is_unchecked() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.int());
        let s = b.new_param(i.string(), Some("s".into()));
        let idx = b.new_temp(i.int());
        let len = b.new_temp(i.int());
        let cmp = b.new_temp(i.bool());
        let ch = b.new_temp(i.char());
        b.assign(
            Place::Local(idx),
            Rvalue::Use(Operand::Const(Const::Int(0))),
        );
        b.assign(
            Place::Local(len),
            Rvalue::StrLen(Operand::Copy(Place::Local(s))),
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
            Place::Local(ch),
            Rvalue::CharAt(
                Operand::Copy(Place::Local(s)),
                Operand::Copy(Place::Local(idx)),
                false,
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
        b.terminate(Terminator::Goto(cond));
        b.switch_to(after);
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(ch)))));
        let mut func = b.finish();
        assert!(Abc.run(&mut func, &i));
        match &func.blocks[body.0 as usize].stmts[0] {
            Statement::Assign(_, Rvalue::CharAt(_, _, true)) => {}
            other => panic!("expected unchecked char_at, got {:?}", other),
        }
    }

    #[test]
    fn interned_string_scan_is_unchecked() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.int());
        let idx = b.new_temp(i.int());
        let len = b.new_temp(i.int());
        let cmp = b.new_temp(i.bool());
        let ch = b.new_temp(i.char());
        let lit = Operand::Const(Const::Str("abc".into()));
        b.assign(
            Place::Local(idx),
            Rvalue::Use(Operand::Const(Const::Int(0))),
        );
        b.assign(Place::Local(len), Rvalue::StrLen(lit.clone()));
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
            Place::Local(ch),
            Rvalue::CharAt(lit, Operand::Copy(Place::Local(idx)), false),
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
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(ch)))));
        let mut func = b.finish();
        assert!(Abc.run(&mut func, &i));
        match &func.blocks[body.0 as usize].stmts[0] {
            Statement::Assign(_, Rvalue::CharAt(_, _, true)) => {}
            other => panic!(
                "expected unchecked char_at on interned string, got {:?}",
                other
            ),
        }
    }
}
