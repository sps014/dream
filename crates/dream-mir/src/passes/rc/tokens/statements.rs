use super::super::is_borrowed_copy;
use super::super::rvalue_reads_local;
use super::calls::move_source;
use super::calls::take_owned_arg_locals;
use crate::Place;
use crate::Statement;
use dream_types::TypeInterner;

pub(crate) fn apply_stmt_tokens(
    stmt: &Statement,
    interner: &TypeInterner,
    is_owned: &dyn Fn(u32) -> bool,
    assign_is_move: bool,
    sink_is_move: impl Fn(u32) -> bool,
    tokens: &mut [bool],
) {
    if let Statement::Assign(Place::Local(dest), rvalue) = stmt
        && is_owned(dest.0) {
            let self_ref = rvalue_reads_local(rvalue, dest.0);
            if !self_ref {
                tokens[dest.0 as usize] = false;
                if is_borrowed_copy(rvalue, interner) {
                    if let Some(src) = move_source(rvalue, is_owned)
                        && assign_is_move {
                            tokens[src.0 as usize] = false;
                        }
                    tokens[dest.0 as usize] = true;
                } else {
                    tokens[dest.0 as usize] = true;
                }
            }
        }
    for local in take_owned_arg_locals(stmt, is_owned) {
        if sink_is_move(local) {
            tokens[local as usize] = false;
        }
    }
    for src in super::super::uniqueness::container_move_locals(stmt) {
        if sink_is_move(src) {
            tokens[src as usize] = false;
        }
    }
}
