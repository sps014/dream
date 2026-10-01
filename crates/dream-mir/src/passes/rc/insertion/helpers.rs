use super::super::{tokens::dest_holds_token, uniqueness::can_unique_destroy};
use crate::{Local, Operand, Place, Statement};
use dream_types::TypeInterner;
use indexmap::IndexSet;
/// Intra-procedural Unique is not object uniqueness: a take param may be a copy the caller
/// still holds (field extract, still-live local). Unique-destroy would `free` under them.
pub(super) fn should_release_leftover(keep: &IndexSet<u32>, n_orig: u32, local: u32) -> bool {
    keep.contains(&local) || local >= n_orig
}

/// Leftover of a funcbox env releases whenever this local still holds the env's token.
/// `funcbox_new` retains the array on the box's behalf, so that is a 2→1 step while the box
/// lives and the typed 1→0 last-drop once it is gone. Gating it on the box's own leftover
/// instead leaked the leaf middleware env, whose box is released in another function.
pub(super) fn leftover_env_ok(local: u32, tokens: &[bool], env_defer: &IndexSet<u32>) -> bool {
    if !env_defer.contains(&local) {
        return true;
    }
    dest_holds_token(tokens, local)
}

pub(super) fn unique_destroy(
    interner: &TypeInterner,
    local_types: &[dream_types::TypeId],
    take_flags: &[bool],
    local: u32,
    unique: bool,
) -> bool {
    unique
        && take_flags.get(local as usize) != Some(&true)
        && local_types
            .get(local as usize)
            .is_some_and(|ty| can_unique_destroy(interner, *ty))
}

pub(super) fn release_one(local: u32, unique: bool) -> Statement {
    let op = Operand::Copy(Place::Local(Local(local)));
    if unique {
        Statement::ReleaseUnique(op)
    } else {
        Statement::Release(op)
    }
}
