//! Last-use unique local stored into a container transfers the +1 (no retain) on both backends.

use crate::{Const, MirFunction, Operand, Place, Rvalue, Statement};
use dream_types::TypeInterner;

/// Skip retain only when `RcInsertion` already transferred the token (`x = null` after the store).
/// Skipping without that null lets a later `Release` destroy a pointer the container still holds.
pub(crate) fn unique_container_move_local(
    func: &MirFunction,
    interner: &TypeInterner,
    value: &Operand,
) -> Option<u32> {
    let Operand::Copy(Place::Local(l)) = value else {
        return None;
    };
    let i = l.0 as usize;
    if i >= func.locals.len() {
        return None;
    }
    let d = &func.locals[i];
    if !interner.is_rc_tracked(d.ty) || d.is_cursor || interner.is_shared_type(d.ty) {
        return None;
    }
    if matches!(interner.kind(d.ty), dream_types::TyKind::Js) {
        return None;
    }
    let is_param = func.params.iter().any(|p| p.0 == l.0);
    if is_param && !d.is_take {
        return None;
    }
    if token_moved_into_container(func, l.0) {
        return Some(l.0);
    }
    None
}

fn token_moved_into_container(func: &MirFunction, local: u32) -> bool {
    for block in &func.blocks {
        for (si, stmt) in block.stmts.iter().enumerate() {
            if !crate::passes::container_move_locals(stmt).contains(&local) {
                continue;
            }
            for later in block.stmts.iter().skip(si + 1) {
                if matches!(
                    later,
                    Statement::Assign(
                        Place::Local(l),
                        Rvalue::Use(Operand::Const(Const::Null))
                    ) if l.0 == local
                ) {
                    return true;
                }
                if matches!(
                    later,
                    Statement::Release(Operand::Copy(Place::Local(l)))
                        | Statement::ReleaseUnique(Operand::Copy(Place::Local(l)))
                        if l.0 == local
                ) {
                    break;
                }
                if is_rc_bookkeeping(later) {
                    continue;
                }
                if crate::passes::stmt_reads_local(later, local) {
                    break;
                }
            }
        }
    }
    false
}

fn is_rc_bookkeeping(stmt: &Statement) -> bool {
    matches!(
        stmt,
        Statement::Retain(_)
            | Statement::Release(_)
            | Statement::ReleaseUnique(_)
            | Statement::ValueDrop(_)
            | Statement::ValueRetain(_)
            | Statement::ValueKill(_)
            | Statement::ForceFree(_)
            | Statement::Nop
            | Statement::DebugLine(_)
            | Statement::SourceLine(_)
    )
}
