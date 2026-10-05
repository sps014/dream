use super::super::is_pure_rvalue;
use super::super::liveness::stmt_reads_local;
use super::super::rvalue_reads_local;
use crate::Const;
use crate::Local;
use crate::MirFunction;
use crate::Operand;
use crate::Place;
use crate::Rvalue;
use crate::Statement;
use crate::Terminator;
use dream_types::TypeInterner;
use indexmap::IndexSet;

/// Owned-RC locals (not cursors, not borrow params). Take-params are owned.
pub(crate) fn is_owned_local(func: &MirFunction, interner: &TypeInterner, local: u32) -> bool {
    let i = local as usize;
    if i >= func.locals.len() {
        return false;
    }
    let d = &func.locals[i];
    if !interner.is_rc_tracked(d.ty) || d.is_cursor {
        return false;
    }
    let is_param = func.params.iter().any(|p| p.0 == local);
    !is_param || d.is_take
}

pub(crate) fn take_param_set(func: &MirFunction) -> IndexSet<u32> {
    func.params
        .iter()
        .copied()
        .filter(|p| func.locals[p.0 as usize].is_take)
        .map(|p| p.0)
        .collect()
}

/// Rebind of an owned dest whose RHS may observe the old pointer (`x = f(x)`, `New`, calls).
/// Lower as `tmp = rhs; Release(x); x = tmp` so the call cannot UAF.
/// Concat / ConcatInt only read their operands; the runtime reuses `dest` in place when unique.
pub(crate) fn needs_rebind_temp(rvalue: &Rvalue, dest: u32) -> bool {
    if rvalue_reads_local(rvalue, dest) {
        return true;
    }
    if matches!(rvalue, Rvalue::Concat(_) | Rvalue::ConcatInt { .. }) {
        return false;
    }
    !is_pure_rvalue(rvalue)
}

pub(crate) fn dest_holds_token(tokens: &[bool], dest: u32) -> bool {
    tokens.get(dest as usize).copied().unwrap_or(false)
}

pub(super) fn reads_local_in_block(block: &crate::BasicBlock, local: u32) -> bool {
    block.stmts.iter().any(|s| stmt_reads_local(s, local))
}

pub(crate) fn terminator_reads_local(term: &Terminator, local: u32) -> bool {
    let mut live = IndexSet::new();
    match term {
        Terminator::If { cond, .. } => add_op(cond, &mut live),
        Terminator::Switch { value, .. } => add_op(value, &mut live),
        Terminator::Return(Some(o)) | Terminator::AsyncComplete(Some(o)) => add_op(o, &mut live),
        Terminator::TailCall { args, .. } => args.iter().for_each(|a| add_op(a, &mut live)),
        Terminator::Await { future, .. } => add_op(future, &mut live),
        _ => {}
    }
    live.contains(&local)
}

pub(super) fn add_op(op: &Operand, live: &mut IndexSet<u32>) {
    if let Operand::Copy(place) = op {
        match place {
            Place::Local(l) => {
                live.insert(l.0);
            }
            Place::Field { base, .. } | Place::Deref { ptr: base, .. } => {
                live.insert(base.0);
            }
            Place::Index { base, index, .. } => {
                live.insert(base.0);
                add_op(index, live);
            }
            Place::Global(_) => {}
        }
    }
}

pub(crate) fn null_local(local: u32) -> Statement {
    Statement::Assign(
        Place::Local(Local(local)),
        Rvalue::Use(Operand::Const(Const::Null)),
    )
}

pub(crate) fn release_and_null(local: u32) -> [Statement; 2] {
    let op = Operand::Copy(Place::Local(Local(local)));
    [
        Statement::Release(op),
        Statement::Assign(
            Place::Local(Local(local)),
            Rvalue::Use(Operand::Const(Const::Null)),
        ),
    ]
}

pub(crate) fn rc_op_on_local(stmt: &Statement, local: u32) -> bool {
    match stmt {
        Statement::Retain(Operand::Copy(Place::Local(l)))
        | Statement::Release(Operand::Copy(Place::Local(l))) => l.0 == local,
        Statement::Assign(Place::Local(l), Rvalue::Use(Operand::Const(Const::Null))) => {
            l.0 == local
        }
        _ => false,
    }
}

pub(crate) fn assigns_local(stmt: &Statement, local: u32) -> bool {
    matches!(stmt, Statement::Assign(Place::Local(l), _) if l.0 == local)
}

pub(crate) fn source_line_end(block: &crate::BasicBlock, si: usize) -> usize {
    let mut end = si;
    for (j, s) in block.stmts.iter().enumerate().skip(si + 1) {
        if matches!(s, Statement::SourceLine(_)) {
            break;
        }
        end = j;
    }
    end
}
