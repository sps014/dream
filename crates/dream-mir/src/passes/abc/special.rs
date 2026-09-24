//! Index shapes beyond `i < len`: a `i * i < len` sieve guard and affine `i * n + j` indices over an
//! `n * n` array.

use super::facts::{arrays_len_at_least, len_bounds, scan, Bound, Defs, Fact, Globals};
use super::{as_local, const_int};
use crate::{BinOp, Const, Local, MirFunction, Operand, Place, Rvalue, Statement};
use std::collections::{BTreeMap, BTreeSet};

/// Largest `n` with `n * n` representable in `int`.
const MAX_SQUARE_SIDE: i64 = 46340;

/// `i * i < len` guarding block `gi` (compare at statement `k`), with `i` a step-1 counter started
/// from a small non-negative constant: the body only runs while `i` is still below `len` (the loop
/// exits at `ceil(sqrt(len))`, before a wrapping product could look small).
pub(super) fn square_guard(
    func: &MirFunction,
    defs: &Defs,
    gi: usize,
    k: usize,
    lhs: &Operand,
    rhs: &Operand,
    out: &mut Vec<Fact>,
) {
    let Some(prod) = as_local(lhs) else {
        return;
    };
    let stmts = &func.blocks[gi].stmts[..k];
    let Some(pos) = stmts
        .iter()
        .rposition(|s| matches!(s, Statement::Assign(Place::Local(d), _) if *d == prod))
    else {
        return;
    };
    let Statement::Assign(
        _,
        Rvalue::Binary(BinOp::Mul, a, b) | Rvalue::CheckedBinary(BinOp::Mul, a, b),
    ) = &stmts[pos]
    else {
        return;
    };
    let (Some(i), Some(i2)) = (as_local(a), as_local(b)) else {
        return;
    };
    if i != i2 || !step_one_counter(func, i) {
        return;
    }
    if stmts[pos + 1..]
        .iter()
        .any(|s| matches!(s, Statement::Assign(Place::Local(d), _) if *d == i))
    {
        return;
    }
    let bounds = len_bounds(func, defs, rhs, Some((gi, k)));
    if bounds.is_empty() {
        return;
    }
    out.push(Fact::NonNeg(i.0));
    for b in bounds {
        out.push(Fact::Below(i.0, b));
    }
}

/// Every definition is a constant in `[0, MAX_SQUARE_SIDE]` or `i + 1`, and at least one constant
/// definition exists. A step of 1 cannot jump from a failing `i * i < len` into the range where the
/// product wraps.
fn step_one_counter(func: &MirFunction, i: Local) -> bool {
    let mut saw_const = false;
    for block in &func.blocks {
        for stmt in &block.stmts {
            let Statement::Assign(Place::Local(d), rv) = stmt else {
                continue;
            };
            if *d != i {
                continue;
            }
            match rv {
                Rvalue::Use(Operand::Const(Const::Int(v))) if (0..=MAX_SQUARE_SIDE).contains(v) => {
                    saw_const = true;
                }
                Rvalue::Binary(BinOp::Add, a, b) | Rvalue::CheckedBinary(BinOp::Add, a, b) => {
                    let step_ok = |op: &Operand| const_int(op) == Some(1);
                    let self_ok = |op: &Operand| as_local(op) == Some(i);
                    if !((self_ok(a) && step_ok(b)) || (self_ok(b) && step_ok(a))) {
                        return false;
                    }
                }
                _ => return false,
            }
        }
    }
    saw_const
}

/// `idx = i * n + j` (or `(i << k) + j` with `n = 1 << k`) with `0 <= i, j < n` at the points the
/// product and the sum are formed: `idx` lies in `[0, n * n)`, so it is in range for every stable
/// array of constant length at least `n * n`. Returns the qualifying single-definition `idx` locals.
pub(super) fn affine_facts(
    func: &MirFunction,
    defs: &Defs,
    entry: &[BTreeSet<Fact>],
    globals: &Globals,
) -> Vec<(u32, Vec<Bound>)> {
    let mut scaled: BTreeMap<u32, i64> = BTreeMap::new();
    scan(func, entry, globals, |_, _, stmt, view| {
        let Statement::Assign(Place::Local(t), rv) = stmt else {
            return;
        };
        if defs.count(t.0) != 1 {
            return;
        }
        let Some((i, n)) = scaled_parts(func, defs, rv) else {
            return;
        };
        if n > 0
            && n <= MAX_SQUARE_SIDE
            && view.holds(&Fact::BelowConst(i.0, n))
            && view.nonneg(i.0)
        {
            scaled.insert(t.0, n);
        }
    });
    if scaled.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    scan(func, entry, globals, |_, _, stmt, view| {
        let Statement::Assign(
            Place::Local(idx),
            Rvalue::Binary(BinOp::Add, a, b) | Rvalue::CheckedBinary(BinOp::Add, a, b),
        ) = stmt
        else {
            return;
        };
        if defs.count(idx.0) != 1 {
            return;
        }
        for (mul, other) in [(a, b), (b, a)] {
            let Some(n) = scaled_root(func, defs, mul, 0).and_then(|t| scaled.get(&t).copied())
            else {
                continue;
            };
            let Some(j) = as_local(other) else {
                continue;
            };
            if !view.holds(&Fact::BelowConst(j.0, n)) || !view.nonneg(j.0) {
                continue;
            }
            let bounds = arrays_len_at_least(func, defs, n * n);
            if !bounds.is_empty() {
                out.push((idx.0, bounds));
                return;
            }
        }
    });
    out
}

/// `local * n` (or `local << k`) as `(local, n)`.
fn scaled_parts(func: &MirFunction, defs: &Defs, rv: &Rvalue) -> Option<(Local, i64)> {
    match rv {
        Rvalue::Binary(BinOp::Mul, a, b) | Rvalue::CheckedBinary(BinOp::Mul, a, b) => {
            if let Some(k) = defs.const_value(func, b) {
                Some((as_local(a)?, k))
            } else {
                Some((as_local(b)?, defs.const_value(func, a)?))
            }
        }
        Rvalue::Binary(BinOp::Shl, a, b) | Rvalue::CheckedBinary(BinOp::Shl, a, b) => {
            let sh = defs.const_value(func, b)?;
            if !(0..31).contains(&sh) {
                return None;
            }
            Some((as_local(a)?, 1_i64 << sh))
        }
        _ => None,
    }
}

/// Follows single-definition copies from `op` to the local holding the product.
fn scaled_root(func: &MirFunction, defs: &Defs, op: &Operand, depth: u32) -> Option<u32> {
    let t = as_local(op)?;
    if depth > 4 || defs.count(t.0) != 1 {
        return None;
    }
    match defs.sole_def(func, t.0)? {
        (Rvalue::Use(src @ Operand::Copy(Place::Local(_))), _) => {
            scaled_root(func, defs, src, depth + 1)
        }
        _ => Some(t.0),
    }
}
