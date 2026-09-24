//! Array and string bounds-check elimination. Marks [`Place::Index`] / [`Rvalue::CharAt`] /
//! [`Rvalue::ByteAt`] `unchecked` when `0 <= idx < len` is proven at that exact statement.
//!
//! Facts are position-precise: a guard (`i < n`, `i <= n - 1`, `i >= 0`, …) establishes a fact on
//! one outgoing edge, the fact holds in every block that edge dominates unless a redefinition of
//! one of its locals can reach the block, and a redefinition inside a block kills it from that
//! statement on. Flow-insensitive facts (non-negative counters, decreasing induction variables,
//! affine `i * n + j` indices) are only derived when *every* definition of the local preserves them.
//! Innermost loops whose bound is not an array length are versioned: a guarded clone runs the body
//! with checks removed when `bound <= len && start >= 0` holds on entry.

mod facts;
mod special;
mod version;

#[cfg(test)]
mod tests;

use super::MirPass;
use crate::{BlockId, Const, Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;
use facts::{Bound, Fact, FactEngine, StrBase};

pub struct Abc;

impl MirPass for Abc {
    fn name(&self) -> &'static str {
        "abc"
    }

    fn run(&self, func: &mut MirFunction, interner: &TypeInterner) -> bool {
        let mut changed = mark_function(func);
        if version::version_one_loop(func, interner) {
            changed = true;
        }
        changed
    }
}

fn mark_function(func: &mut MirFunction) -> bool {
    if !has_checked_access(func) {
        return false;
    }
    let engine = FactEngine::new(func);
    let mut changed = false;
    for (bi, block) in func.blocks.iter_mut().enumerate() {
        let mut view = engine.entry_view(bi);
        for stmt in &mut block.stmts {
            changed |= mark_stmt(stmt, &view);
            view.kill_defs(stmt);
        }
        changed |= mark_terminator(&mut block.terminator, &view);
    }
    changed
}

fn has_checked_access(func: &MirFunction) -> bool {
    let mut found = false;
    for block in &func.blocks {
        for stmt in &block.stmts {
            visit_stmt_accesses(stmt, &mut |a| {
                found |= !a.unchecked();
            });
        }
        visit_terminator_accesses(&block.terminator, &mut |a| {
            found |= !a.unchecked();
        });
    }
    found
}

/// One bounds-checked access site, viewed without mutation.
enum Access<'a> {
    Index {
        base: Local,
        index: &'a Operand,
        unchecked: bool,
    },
    Char(&'a Operand, &'a Operand, bool),
    Byte(&'a Operand, &'a Operand, bool),
}

impl Access<'_> {
    fn unchecked(&self) -> bool {
        match self {
            Access::Index { unchecked, .. } => *unchecked,
            Access::Char(_, _, u) | Access::Byte(_, _, u) => *u,
        }
    }

    /// The fact pair that makes this access in range, if its index is a plain local.
    fn required(&self) -> Option<(Local, Bound)> {
        match self {
            Access::Index { base, index, .. } => Some((as_local(index)?, Bound::Arr(base.0))),
            Access::Char(s, i, _) => Some((as_local(i)?, Bound::Unit(str_base(s)?))),
            Access::Byte(s, i, _) => Some((as_local(i)?, Bound::Byte(str_base(s)?))),
        }
    }
}

fn visit_place_accesses<'a>(place: &'a Place, f: &mut impl FnMut(Access<'a>)) {
    if let Place::Index {
        base,
        index,
        unchecked,
    } = place
    {
        f(Access::Index {
            base: *base,
            index,
            unchecked: *unchecked,
        });
        visit_operand_accesses(index, f);
    }
}

fn visit_operand_accesses<'a>(op: &'a Operand, f: &mut impl FnMut(Access<'a>)) {
    if let Operand::Copy(p) = op {
        visit_place_accesses(p, f);
    }
}

fn visit_rvalue_accesses<'a>(rv: &'a Rvalue, f: &mut impl FnMut(Access<'a>)) {
    match rv {
        Rvalue::CharAt(s, i, u) => {
            f(Access::Char(s, i, *u));
            visit_operand_accesses(s, f);
            visit_operand_accesses(i, f);
        }
        Rvalue::ByteAt(s, i, u) => {
            f(Access::Byte(s, i, *u));
            visit_operand_accesses(s, f);
            visit_operand_accesses(i, f);
        }
        other => {
            for op in rvalue_operands(other) {
                visit_operand_accesses(op, f);
            }
        }
    }
}

fn visit_stmt_accesses<'a>(stmt: &'a Statement, f: &mut impl FnMut(Access<'a>)) {
    match stmt {
        Statement::Assign(place, rv) => {
            visit_place_accesses(place, f);
            visit_rvalue_accesses(rv, f);
        }
        Statement::Call { args, .. }
        | Statement::IndirectCall { args, .. }
        | Statement::InterfaceCall { args, .. } => {
            for a in args {
                visit_operand_accesses(a, f);
            }
        }
        _ => {}
    }
}

fn visit_terminator_accesses<'a>(t: &'a Terminator, f: &mut impl FnMut(Access<'a>)) {
    match t {
        Terminator::If { cond: o, .. }
        | Terminator::Return(Some(o))
        | Terminator::AsyncComplete(Some(o))
        | Terminator::Switch { value: o, .. }
        | Terminator::Await { future: o, .. } => visit_operand_accesses(o, f),
        Terminator::TailCall { args, .. } => {
            for a in args {
                visit_operand_accesses(a, f);
            }
        }
        _ => {}
    }
}

/// The plain operands of the rvalue shapes that can carry an index place (everything except
/// `CharAt`/`ByteAt`, which [`visit_rvalue_accesses`] handles itself).
fn rvalue_operands(rv: &Rvalue) -> Vec<&Operand> {
    match rv {
        Rvalue::Use(o) | Rvalue::Unary(_, o) | Rvalue::CheckedNeg(o) | Rvalue::ArrayLen(o) => {
            vec![o]
        }
        Rvalue::Binary(_, a, b) | Rvalue::CheckedBinary(_, a, b) => vec![a, b],
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => vec![cond, then_val, else_val],
        Rvalue::Call { args, .. } | Rvalue::New { args, .. } => args.iter().collect(),
        Rvalue::InterfaceCall { receiver, args, .. } => {
            let mut v = vec![receiver];
            v.extend(args.iter());
            v
        }
        _ => Vec::new(),
    }
}

fn access_in_range(a: &Access<'_>, facts: &facts::FactView<'_>) -> bool {
    if a.unchecked() {
        return false;
    }
    match a.required() {
        Some((idx, bound)) => facts.holds(&Fact::Below(idx.0, bound)) && facts.nonneg(idx.0),
        None => false,
    }
}

fn mark_stmt(stmt: &mut Statement, facts: &facts::FactView<'_>) -> bool {
    match stmt {
        Statement::Assign(place, rv) => mark_place(place, facts) | mark_rvalue(rv, facts),
        Statement::Call { args, .. }
        | Statement::IndirectCall { args, .. }
        | Statement::InterfaceCall { args, .. } => {
            let mut c = false;
            for a in args {
                c |= mark_operand(a, facts);
            }
            c
        }
        _ => false,
    }
}

fn mark_terminator(t: &mut Terminator, facts: &facts::FactView<'_>) -> bool {
    match t {
        Terminator::If { cond: o, .. }
        | Terminator::Return(Some(o))
        | Terminator::AsyncComplete(Some(o))
        | Terminator::Switch { value: o, .. }
        | Terminator::Await { future: o, .. } => mark_operand(o, facts),
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

fn mark_rvalue(rv: &mut Rvalue, facts: &facts::FactView<'_>) -> bool {
    match rv {
        Rvalue::CharAt(s, i, unchecked) => {
            let mut c = mark_operand(s, facts) | mark_operand(i, facts);
            if !*unchecked && access_in_range(&Access::Char(s, i, false), facts) {
                *unchecked = true;
                c = true;
            }
            c
        }
        Rvalue::ByteAt(s, i, unchecked) => {
            let mut c = mark_operand(s, facts) | mark_operand(i, facts);
            if !*unchecked && access_in_range(&Access::Byte(s, i, false), facts) {
                *unchecked = true;
                c = true;
            }
            c
        }
        Rvalue::Use(o) | Rvalue::Unary(_, o) | Rvalue::CheckedNeg(o) | Rvalue::ArrayLen(o) => {
            mark_operand(o, facts)
        }
        Rvalue::Binary(_, a, b) | Rvalue::CheckedBinary(_, a, b) => {
            mark_operand(a, facts) | mark_operand(b, facts)
        }
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

fn mark_operand(op: &mut Operand, facts: &facts::FactView<'_>) -> bool {
    match op {
        Operand::Copy(p) => mark_place(p, facts),
        Operand::Const(_) => false,
    }
}

fn mark_place(place: &mut Place, facts: &facts::FactView<'_>) -> bool {
    let Place::Index {
        base,
        index,
        unchecked,
    } = place
    else {
        return false;
    };
    let mut c = mark_operand(index, facts);
    if !*unchecked {
        let a = Access::Index {
            base: *base,
            index,
            unchecked: false,
        };
        if access_in_range(&a, facts) {
            *unchecked = true;
            c = true;
        }
    }
    c
}

fn as_local(op: &Operand) -> Option<Local> {
    match op {
        Operand::Copy(Place::Local(l)) => Some(*l),
        _ => None,
    }
}

fn str_base(op: &Operand) -> Option<StrBase> {
    match op {
        Operand::Copy(Place::Local(l)) => Some(StrBase::Local(l.0)),
        Operand::Const(Const::Str(s)) => Some(StrBase::Lit(s.clone())),
        _ => None,
    }
}

fn const_int(op: &Operand) -> Option<i64> {
    match op {
        Operand::Const(Const::Int(v)) => Some(*v),
        _ => None,
    }
}

fn block_id(i: usize) -> BlockId {
    BlockId(i as u32)
}
