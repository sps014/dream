//! Drops `Retain`/`Release` of string literals. Literal blocks are immortal `constant` globals
//! whose count word the runtime never writes, so both operations are no-ops; the runtime's
//! relaxed atomic load of that word is the one part LLVM cannot fold away.

use crate::{Const, MirFunction, Operand, Place, Rvalue, Statement, Terminator};

pub(super) fn elide_literal_rc(func: &mut MirFunction) -> bool {
    let literal = literal_locals(func);
    let mut changed = false;
    for block in &mut func.blocks {
        for stmt in &mut block.stmts {
            let (Statement::Retain(op) | Statement::Release(op)) = stmt else {
                continue;
            };
            let is_literal = match op {
                Operand::Const(Const::Str(_)) => true,
                Operand::Copy(Place::Local(l)) => literal.get(l.0 as usize) == Some(&true),
                _ => false,
            };
            if is_literal {
                *stmt = Statement::Nop;
                changed = true;
            }
        }
    }
    changed
}

/// Non-parameter locals every definition of which is a string literal or `null`, with at least
/// one literal definition.
fn literal_locals(func: &MirFunction) -> Vec<bool> {
    let n = func.locals.len();
    let mut saw_literal = vec![false; n];
    let mut other = vec![false; n];
    let mark = |v: &mut Vec<bool>, l: crate::Local| {
        if let Some(x) = v.get_mut(l.0 as usize) {
            *x = true;
        }
    };
    for p in &func.params {
        mark(&mut other, *p);
    }
    for block in &func.blocks {
        for stmt in &block.stmts {
            let Statement::Assign(Place::Local(d), rv) = stmt else {
                continue;
            };
            match rv {
                Rvalue::Use(Operand::Const(Const::Str(_))) => mark(&mut saw_literal, *d),
                Rvalue::Use(Operand::Const(Const::Null)) => {}
                _ => mark(&mut other, *d),
            }
        }
        if let Terminator::Await { dest: Some(d), .. } = &block.terminator {
            mark(&mut other, *d);
        }
    }
    (0..n).map(|l| saw_literal[l] && !other[l]).collect()
}
