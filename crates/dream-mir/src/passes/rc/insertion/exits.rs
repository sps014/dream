use super::super::{
    tokens::{is_owned_local, leftover_keep, leftover_order, null_local, release_and_null},
    value::insert_complete_value_drops,
};
use super::{
    helpers::{leftover_env_ok, should_release_leftover},
    prepare::State,
};
use crate::{Local, LocalDecl, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;

pub(super) fn insert(
    func: &mut MirFunction,
    interner: &TypeInterner,
    state: &State,
    changed: &mut bool,
) {
    let analysis = &state.analysis;
    let leftover_parent = &state.leftover_parent;
    let env_defer = &state.env_defer;
    let n_orig = state.n_orig;
    let is_owned = |l: u32| state.owned_flags.get(l as usize).copied().unwrap_or(false);
    let ret_is_ref = interner.is_rc_tracked(func.ret);
    let mut spills: Vec<LocalDecl> = Vec::new();
    let next_local = func.locals.len() as u32;
    for bi in 0..func.blocks.len() {
        let ret = match &func.blocks[bi].terminator {
            Terminator::Return(v) | Terminator::AsyncComplete(v) => v.clone(),
            _ => continue,
        };
        let is_async_complete = matches!(func.blocks[bi].terminator, Terminator::AsyncComplete(_));
        let (skip, spill_from): (Option<u32>, Option<Operand>) = match &ret {
            Some(Operand::Copy(Place::Local(l))) if is_owned(l.0) => (Some(l.0), None),
            Some(op) if ret_is_ref => (None, Some(op.clone())),
            _ => (None, None),
        };
        let skip = if let Some(op) = spill_from {
            let temp = Local(next_local + spills.len() as u32);
            spills.push(LocalDecl {
                ty: func.ret,
                name: None,
                is_ref: false,
                is_take: false,
                is_cursor: false,
                manual_drop: false,
            });
            func.blocks[bi]
                .stmts
                .push(Statement::Assign(Place::Local(temp), Rvalue::Use(op)));
            func.blocks[bi]
                .stmts
                .push(Statement::Retain(Operand::Copy(Place::Local(temp))));
            let spilled = Some(Operand::Copy(Place::Local(temp)));
            func.blocks[bi].terminator = if is_async_complete {
                Terminator::AsyncComplete(spilled)
            } else {
                Terminator::Return(spilled)
            };
            *changed = true;
            Some(temp.0)
        } else {
            skip
        };
        if is_async_complete {
            insert_complete_value_drops(func, interner, bi, skip, changed);
        }
        // Tokens still owned at Return/AsyncComplete (not destroyed earlier) are released here.
        // Classes already dropped on the arms; sweeping every RC local would double-free
        // at joins (`unbalanced_if_releases_on_kept_arm`).
        let tokens = analysis.token_out.get(bi);
        let nloc = func.locals.len().min(tokens.map(|t| t.len()).unwrap_or(0));
        let mut ret_drop: Vec<u32> = Vec::new();
        for i in 0..nloc {
            let local = i as u32;
            if Some(local) == skip || !is_owned_local(func, interner, local) {
                continue;
            }
            if !tokens.and_then(|row| row.get(i).copied()).unwrap_or(false) {
                continue;
            }
            // A last-use field/index store in *this* block already took the token.
            // Loop-carried `token_out` can still look owned after that move. A sink in
            // another block must not suppress leftover here (last-iteration future/funcbox).
            if analysis
                .sink_move
                .iter()
                .any(|&(b, _, l)| b == bi && l == local)
            {
                continue;
            }
            ret_drop.push(local);
        }
        let ret_keep = leftover_keep(func, ret_drop.iter().copied());
        let token_row = tokens.cloned().unwrap_or_default();
        for local in leftover_order(leftover_parent, ret_drop.iter().copied(), env_defer) {
            if should_release_leftover(&ret_keep, n_orig, local)
                && leftover_env_ok(local, &token_row, env_defer)
            {
                func.blocks[bi].stmts.extend(release_and_null(local, false));
            } else {
                func.blocks[bi].stmts.push(null_local(local));
            }
            *changed = true;
        }
    }
    func.locals.extend(spills);
}
