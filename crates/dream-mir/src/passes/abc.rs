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
        note_square_index(func, nonneg, bi, idx, &arrs, &dom, &mut arr_facts);
    }
    note_affine(func, nonneg, &dom, &mut arr_facts);
    (arr_facts, char_facts, byte_facts)
}

/// `i * i < len` with `i` a step-1 counter started from a non-negative constant: the body only
/// runs while `i` is still below `len` (the loop exits at `ceil(sqrt(len))`, before a wrapping
/// product could look small).
fn note_square_index(
    func: &MirFunction,
    nonneg: &HashSet<u32>,
    guard: usize,
    prod: Local,
    arrs: &[u32],
    dom: &DomTree,
    arr_facts: &mut ArrFacts,
) {
    if arrs.is_empty() {
        return;
    }
    let Some(i) = squared_local(func, guard, prod) else {
        return;
    };
    if !nonneg.contains(&i.0) || !step_one_counter(func, i) {
        return;
    }
    let header = crate::BlockId(guard as u32);
    for (ti, _) in func.blocks.iter().enumerate() {
        let t = crate::BlockId(ti as u32);
        let then_dom = match &func.blocks[guard].terminator {
            Terminator::If { then_blk, .. } => t == *then_blk || dom.dominates(*then_blk, t),
            _ => false,
        };
        if then_dom && !redefines_between(func, dom, header, t, &[i]) {
            for &arr in arrs {
                arr_facts[ti].insert((i.0, arr));
            }
        }
    }
}

fn squared_local(func: &MirFunction, block: usize, prod: Local) -> Option<Local> {
    let rv = last_def_in_block(func, block, prod)?;
    match rv {
        Rvalue::Binary(BinOp::Mul, a, b) | Rvalue::CheckedBinary(BinOp::Mul, a, b) => {
            let ia = as_local(a)?;
            let ib = as_local(b)?;
            (ia == ib).then_some(ia)
        }
        _ => None,
    }
}

/// Every definition is a non-negative constant or `i + 1`, and at least one constant def exists.
/// A step of 1 cannot jump from a failing `i * i < len` into the range where the product wraps.
fn step_one_counter(func: &MirFunction, i: Local) -> bool {
    let mut saw_const = false;
    let mut saw = false;
    for block in &func.blocks {
        for stmt in &block.stmts {
            let Statement::Assign(Place::Local(d), rv) = stmt else {
                continue;
            };
            if *d != i {
                continue;
            }
            saw = true;
            match rv {
                Rvalue::Use(Operand::Const(Const::Int(v))) if *v >= 0 && *v <= 46340 => {
                    saw_const = true;
                }
                Rvalue::Binary(BinOp::Add, a, b) | Rvalue::CheckedBinary(BinOp::Add, a, b) => {
                    let step_ok = |op: &Operand| matches!(op, Operand::Const(Const::Int(1)));
                    let self_ok = |op: &Operand| as_local(op) == Some(i);
                    if !((self_ok(a) && step_ok(b)) || (self_ok(b) && step_ok(a))) {
                        return false;
                    }
                }
                _ => return false,
            }
        }
    }
    saw && saw_const
}

/// `idx = i * n + j` is in range for an array of length `n * n` when `0 <= i,j < n` and `n * n`
/// fits in `int`.
fn note_affine(
    func: &MirFunction,
    nonneg: &HashSet<u32>,
    dom: &DomTree,
    arr_facts: &mut ArrFacts,
) {
    for (bi, block) in func.blocks.iter().enumerate() {
        for stmt in &block.stmts {
            let Statement::Assign(Place::Local(idx), rv) = stmt else {
                continue;
            };
            if func
                .blocks
                .iter()
                .flat_map(|b| &b.stmts)
                .filter(|s| matches!(s, Statement::Assign(Place::Local(d), _) if d == idx))
                .count()
                != 1
            {
                continue;
            }
            let (Rvalue::Binary(BinOp::Add, a, b) | Rvalue::CheckedBinary(BinOp::Add, a, b)) = rv
            else {
                continue;
            };
            let Some((i, n, j)) = affine_parts(func, a, b).or_else(|| affine_parts(func, b, a))
            else {
                continue;
            };
            if n <= 0 || n > 46340 || !nonneg.contains(&i.0) || !nonneg.contains(&j.0) {
                continue;
            }
            let len = n.saturating_mul(n);
            let arrs = arrays_of_len_at_least(func, len);
            if arrs.is_empty() {
                continue;
            }
            let Some(gi) = guard_block(func, i, n) else {
                continue;
            };
            let Some(gj) = guard_block(func, j, n) else {
                continue;
            };
            let use_b = crate::BlockId(bi as u32);
            if !guard_covers(func, dom, gi, use_b, i) || !guard_covers(func, dom, gj, use_b, j) {
                continue;
            }
            for arr in arrs {
                arr_facts[bi].insert((idx.0, arr));
            }
        }
    }
}

fn affine_parts(func: &MirFunction, mul: &Operand, other: &Operand) -> Option<(Local, i64, Local)> {
    let j = as_local(other)?;
    let (i, n) = match mul {
        Operand::Copy(Place::Local(t)) => mul_local(func, *t)?,
        _ => return None,
    };
    Some((i, n, j))
}

fn mul_local(func: &MirFunction, t: Local) -> Option<(Local, i64)> {
    scaled_local(func, t, 0)
}

/// `t` as `local * scale`. Follows a single copy, a multiply by a constant, and a shift that
/// algebraic simplification uses for a power-of-two multiply (`i * 64` → `i << 6`).
fn scaled_local(func: &MirFunction, t: Local, depth: u32) -> Option<(Local, i64)> {
    if depth > 4 {
        return None;
    }
    let rv = sole_rvalue(func, t)?;
    match rv {
        Rvalue::Use(Operand::Copy(Place::Local(s))) => scaled_local(func, *s, depth + 1),
        Rvalue::Binary(BinOp::Mul, a, b) | Rvalue::CheckedBinary(BinOp::Mul, a, b) => {
            let (local, scale) = if let Some(k) = const_operand(func, b) {
                (as_local(a)?, k)
            } else if let Some(k) = const_operand(func, a) {
                (as_local(b)?, k)
            } else {
                return None;
            };
            Some((local, scale))
        }
        Rvalue::Binary(BinOp::Shl, a, b) | Rvalue::CheckedBinary(BinOp::Shl, a, b) => {
            let sh = const_operand(func, b)?;
            if !(0..31).contains(&sh) {
                return None;
            }
            Some((as_local(a)?, 1_i64 << sh))
        }
        _ => None,
    }
}

fn sole_rvalue(func: &MirFunction, local: Local) -> Option<&Rvalue> {
    let mut found = None;
    for block in &func.blocks {
        for stmt in &block.stmts {
            let Statement::Assign(Place::Local(d), rv) = stmt else {
                continue;
            };
            if *d != local {
                continue;
            }
            if found.is_some() {
                return None;
            }
            found = Some(rv);
        }
    }
    found
}

fn const_operand(func: &MirFunction, op: &Operand) -> Option<i64> {
    match op {
        Operand::Const(Const::Int(v)) => Some(*v),
        Operand::Copy(Place::Local(l)) => sole_const(func, *l),
        _ => None,
    }
}

fn sole_const(func: &MirFunction, local: Local) -> Option<i64> {
    let mut k = None;
    for block in &func.blocks {
        for stmt in &block.stmts {
            let Statement::Assign(Place::Local(d), rv) = stmt else {
                continue;
            };
            if *d != local {
                continue;
            }
            let Rvalue::Use(Operand::Const(Const::Int(v))) = rv else {
                return None;
            };
            if k.is_some_and(|prev| prev != *v) {
                return None;
            }
            k = Some(*v);
        }
    }
    k
}

fn arrays_of_len_at_least(func: &MirFunction, len: i64) -> Vec<u32> {
    let mut arrs = Vec::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            if let Statement::Assign(Place::Local(arr), Rvalue::ArrayNew { len: n, .. }) = stmt {
                let k = match n {
                    Operand::Const(Const::Int(v)) => Some(*v),
                    Operand::Copy(Place::Local(l)) => sole_const(func, *l),
                    _ => None,
                };
                if k.is_some_and(|k| k >= len) {
                    arrs.push(arr.0);
                }
            }
        }
    }
    arrs
}

fn guard_block(func: &MirFunction, local: Local, n: i64) -> Option<usize> {
    for (bi, block) in func.blocks.iter().enumerate() {
        let Terminator::If {
            cond: Operand::Copy(Place::Local(cmp)),
            ..
        } = &block.terminator
        else {
            continue;
        };
        let Some((idx, bound)) = lt_bound(block, *cmp) else {
            continue;
        };
        if idx != local {
            continue;
        }
        let bound_k = match &bound {
            Operand::Const(Const::Int(v)) => Some(*v),
            Operand::Copy(Place::Local(l)) => sole_const(func, *l),
            _ => None,
        };
        if bound_k == Some(n) {
            return Some(bi);
        }
    }
    None
}

fn guard_covers(
    func: &MirFunction,
    dom: &DomTree,
    guard: usize,
    use_b: crate::BlockId,
    local: Local,
) -> bool {
    let header = crate::BlockId(guard as u32);
    let then_blk = match &func.blocks[guard].terminator {
        Terminator::If { then_blk, .. } => *then_blk,
        _ => return false,
    };
    (use_b == then_blk || dom.dominates(then_blk, use_b))
        && !redefines_between(func, dom, header, use_b, &[local])
}

fn last_def_in_block(func: &MirFunction, block: usize, local: Local) -> Option<&Rvalue> {
    func.blocks[block].stmts.iter().rev().find_map(|s| match s {
        Statement::Assign(Place::Local(d), rv) if *d == local => Some(rv),
        _ => None,
    })
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
        Rvalue::Use(o) | Rvalue::Unary(_, o) | Rvalue::CheckedNeg(o) | Rvalue::ArrayLen(o) => {
            mark_operand(o, arr_facts, bi)
        }
        Rvalue::Binary(_, a, b) | Rvalue::CheckedBinary(_, a, b) => {
            mark_operand(a, arr_facts, bi) | mark_operand(b, arr_facts, bi)
        }
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
                    Rvalue::Binary(BinOp::Add | BinOp::Mul, a, b)
                    | Rvalue::CheckedBinary(BinOp::Add | BinOp::Mul, a, b) => {
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
        if b == from || b == to {
            continue;
        }
        // A write reaches `to` only when it dominates `to`. A loop latch dominated by `to`
        // stores the next iteration, which re-enters through `from` and is checked again.
        if !(dom.dominates(from, b) && dom.dominates(b, to)) {
            continue;
        }
        for stmt in &block.stmts {
            if let Statement::Assign(Place::Local(d), _) = stmt {
                if locals.iter().any(|l| l.0 == d.0) {
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

    #[test]
    fn affine_index_is_unchecked() {
        let mut i = TypeInterner::new();
        let arr_ty = i.array(i.float());
        let mut b = FunctionBuilder::new("f", i.int());
        let arr = b.new_temp(arr_ty);
        let n = b.new_temp(i.int());
        let iv = b.new_temp(i.int());
        let j = b.new_temp(i.int());
        let mul = b.new_temp(i.int());
        let idx = b.new_temp(i.int());
        let ci = b.new_temp(i.bool());
        let cj = b.new_temp(i.bool());
        let elem = b.new_temp(i.float());
        b.assign(
            Place::Local(n),
            Rvalue::Use(Operand::Const(Const::Int(64))),
        );
        b.assign(
            Place::Local(arr),
            Rvalue::ArrayNew {
                elem_ty: i.float(),
                len: Operand::Const(Const::Int(4096)),
            },
        );
        b.assign(
            Place::Local(iv),
            Rvalue::Use(Operand::Const(Const::Int(0))),
        );
        let icond = b.new_block();
        let ibody = b.new_block();
        let after = b.new_block();
        b.terminate(Terminator::Goto(icond));
        b.switch_to(icond);
        b.assign(
            Place::Local(ci),
            Rvalue::Binary(
                BinOp::Lt,
                Operand::Copy(Place::Local(iv)),
                Operand::Copy(Place::Local(n)),
            ),
        );
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(ci)),
            then_blk: ibody,
            else_blk: after,
        });
        b.switch_to(ibody);
        b.assign(
            Place::Local(j),
            Rvalue::Use(Operand::Const(Const::Int(0))),
        );
        let jcond = b.new_block();
        let jbody = b.new_block();
        let ilatch = b.new_block();
        b.terminate(Terminator::Goto(jcond));
        b.switch_to(jcond);
        b.assign(
            Place::Local(cj),
            Rvalue::Binary(
                BinOp::Lt,
                Operand::Copy(Place::Local(j)),
                Operand::Const(Const::Int(64)),
            ),
        );
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(cj)),
            then_blk: jbody,
            else_blk: ilatch,
        });
        b.switch_to(jbody);
        b.assign(
            Place::Local(mul),
            Rvalue::Binary(
                BinOp::Mul,
                Operand::Copy(Place::Local(iv)),
                Operand::Copy(Place::Local(n)),
            ),
        );
        b.assign(
            Place::Local(idx),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(mul)),
                Operand::Copy(Place::Local(j)),
            ),
        );
        b.assign(
            Place::Local(elem),
            Rvalue::Use(Operand::Copy(Place::index(
                arr,
                Operand::Copy(Place::Local(idx)),
            ))),
        );
        b.assign(
            Place::Local(j),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(j)),
                Operand::Const(Const::Int(1)),
            ),
        );
        b.terminate(Terminator::Goto(jcond));
        b.switch_to(ilatch);
        b.assign(
            Place::Local(iv),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(iv)),
                Operand::Const(Const::Int(1)),
            ),
        );
        b.terminate(Terminator::Goto(icond));
        b.switch_to(after);
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(elem)))));
        let mut func = b.finish();
        assert!(Abc.run(&mut func, &i));
        let unchecked = func.blocks[jbody.0 as usize].stmts.iter().any(|s| {
            matches!(
                s,
                Statement::Assign(_, Rvalue::Use(Operand::Copy(Place::Index { unchecked: true, .. })))
            )
        });
        assert!(unchecked, "i * n + j must drop the bounds check");
    }

    /// `idx = (i << 6) + k` in a loop whose `k = k + 1` lives in a latch the body dominates.
    /// The latch stores the next iteration; it must not hide the `k < 64` guard.
    #[test]
    fn shift_affine_index_with_latch_is_unchecked() {
        let mut i = TypeInterner::new();
        let arr_ty = i.array(i.float());
        let mut b = FunctionBuilder::new("f", i.int());
        let arr = b.new_temp(arr_ty);
        let iv = b.new_temp(i.int());
        let k = b.new_temp(i.int());
        let scale = b.new_temp(i.int());
        let idx = b.new_temp(i.int());
        let ci = b.new_temp(i.bool());
        let ck = b.new_temp(i.bool());
        let elem = b.new_temp(i.float());
        b.assign(
            Place::Local(arr),
            Rvalue::ArrayNew {
                elem_ty: i.float(),
                len: Operand::Const(Const::Int(4096)),
            },
        );
        b.assign(
            Place::Local(iv),
            Rvalue::Use(Operand::Const(Const::Int(0))),
        );
        let icond = b.new_block();
        let ibody = b.new_block();
        let after = b.new_block();
        b.terminate(Terminator::Goto(icond));
        b.switch_to(icond);
        b.assign(
            Place::Local(ci),
            Rvalue::Binary(
                BinOp::Lt,
                Operand::Copy(Place::Local(iv)),
                Operand::Const(Const::Int(64)),
            ),
        );
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(ci)),
            then_blk: ibody,
            else_blk: after,
        });
        b.switch_to(ibody);
        b.assign(
            Place::Local(scale),
            Rvalue::Binary(
                BinOp::Shl,
                Operand::Copy(Place::Local(iv)),
                Operand::Const(Const::Int(6)),
            ),
        );
        b.assign(
            Place::Local(k),
            Rvalue::Use(Operand::Const(Const::Int(0))),
        );
        let kcond = b.new_block();
        let kbody = b.new_block();
        let klatch = b.new_block();
        let ilatch = b.new_block();
        b.terminate(Terminator::Goto(kcond));
        b.switch_to(kcond);
        b.assign(
            Place::Local(ck),
            Rvalue::Binary(
                BinOp::Lt,
                Operand::Copy(Place::Local(k)),
                Operand::Const(Const::Int(64)),
            ),
        );
        b.terminate(Terminator::If {
            cond: Operand::Copy(Place::Local(ck)),
            then_blk: kbody,
            else_blk: ilatch,
        });
        b.switch_to(kbody);
        b.assign(
            Place::Local(idx),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(scale)),
                Operand::Copy(Place::Local(k)),
            ),
        );
        b.assign(
            Place::Local(elem),
            Rvalue::Use(Operand::Copy(Place::index(
                arr,
                Operand::Copy(Place::Local(idx)),
            ))),
        );
        b.terminate(Terminator::Goto(klatch));
        b.switch_to(klatch);
        b.assign(
            Place::Local(k),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(k)),
                Operand::Const(Const::Int(1)),
            ),
        );
        b.terminate(Terminator::Goto(kcond));
        b.switch_to(ilatch);
        b.assign(
            Place::Local(iv),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(iv)),
                Operand::Const(Const::Int(1)),
            ),
        );
        b.terminate(Terminator::Goto(icond));
        b.switch_to(after);
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(elem)))));
        let mut func = b.finish();
        assert!(Abc.run(&mut func, &i));
        let unchecked = func.blocks[kbody.0 as usize].stmts.iter().any(|s| {
            matches!(
                s,
                Statement::Assign(_, Rvalue::Use(Operand::Copy(Place::Index { unchecked: true, .. })))
            )
        });
        assert!(unchecked, "i << 6 + k must drop the bounds check");
    }

    #[test]
    fn square_bound_index_is_unchecked() {
        let mut i = TypeInterner::new();
        let arr_ty = i.array(i.int());
        let mut b = FunctionBuilder::new("f", i.int());
        let arr = b.new_temp(arr_ty);
        let iv = b.new_temp(i.int());
        let sq = b.new_temp(i.int());
        let cmp = b.new_temp(i.bool());
        let elem = b.new_temp(i.int());
        b.assign(
            Place::Local(arr),
            Rvalue::ArrayNew {
                elem_ty: i.int(),
                len: Operand::Const(Const::Int(4096)),
            },
        );
        b.assign(
            Place::Local(iv),
            Rvalue::Use(Operand::Const(Const::Int(2))),
        );
        let cond = b.new_block();
        let body = b.new_block();
        let after = b.new_block();
        b.terminate(Terminator::Goto(cond));
        b.switch_to(cond);
        b.assign(
            Place::Local(sq),
            Rvalue::Binary(
                BinOp::Mul,
                Operand::Copy(Place::Local(iv)),
                Operand::Copy(Place::Local(iv)),
            ),
        );
        b.assign(
            Place::Local(cmp),
            Rvalue::Binary(
                BinOp::Lt,
                Operand::Copy(Place::Local(sq)),
                Operand::Const(Const::Int(4096)),
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
                Operand::Copy(Place::Local(iv)),
            ))),
        );
        b.assign(
            Place::Local(iv),
            Rvalue::Binary(
                BinOp::Add,
                Operand::Copy(Place::Local(iv)),
                Operand::Const(Const::Int(1)),
            ),
        );
        b.terminate(Terminator::Goto(cond));
        b.switch_to(after);
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(elem)))));
        let mut func = b.finish();
        assert!(Abc.run(&mut func, &i));
        let unchecked = func.blocks[body.0 as usize].stmts.iter().any(|s| {
            matches!(
                s,
                Statement::Assign(_, Rvalue::Use(Operand::Copy(Place::Index { unchecked: true, .. })))
            )
        });
        assert!(unchecked, "i under i * i < len must drop the bounds check");
    }
}
