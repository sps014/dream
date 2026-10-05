use super::eligibility::Site;
use super::remap::arg_type;
use super::remap::remap_block;
use super::remap::wasm_kind;
use crate::BasicBlock;
use crate::BlockId;
use crate::Const;
use crate::Local;
use crate::LocalDecl;
use crate::Operand;
use crate::Place;
use crate::Rvalue;
use crate::Statement;
use crate::Terminator;
use dream_types::TypeInterner;
use indexmap::IndexSet as HashSet;

/// Remaps a callee [`LocalDecl`] into the caller using the callee's [`ValueFrame`] classification:
/// borrows stay aliases (`is_ref`); Param/Owning get `manual_drop` so call-site [`Statement::ValueDrop`]
/// owns teardown (caller frame exit must not drop them again).
fn remap_local_decl(
    decl: &LocalDecl,
    kind: Option<crate::backend::shared::ValueLocalKind>,
) -> LocalDecl {
    let mut d = decl.clone();
    match kind {
        Some(crate::backend::shared::ValueLocalKind::Borrow) => {
            d.is_ref = true;
            d.manual_drop = false;
        }
        Some(
            crate::backend::shared::ValueLocalKind::Param
            | crate::backend::shared::ValueLocalKind::Owning,
        ) => {
            d.manual_drop = true;
            d.is_ref = false;
            if d.name.is_none() {
                d.name = Some("__vinl".into());
            }
        }
        None => {}
    }
    d
}

/// Performs the inline described by `site` into function `fi`.
pub(super) fn perform_inline(mir: &mut crate::Mir, fi: usize, site: Site, interner: &TypeInterner) {
    // Clone the callee's shape before mutating the caller (they share `mir.functions`).
    let (g_entry, g_params, g_locals, g_blocks) = {
        let g = &mir.functions[site.callee];
        (
            g.entry,
            g.params.clone(),
            g.locals.clone(),
            g.blocks.clone(),
        )
    };

    // Classify value locals on the *callee* before remapping — borrow aliases must not be
    // reclassified as owning in the caller. Locals already marked `manual_drop` (from a prior
    // inline into this callee) keep their existing `ValueDrop` in the remapped body; do not drop
    // them again at this site's continuation.
    let callee_frame =
        crate::backend::shared::ValueFrame::compute(&mir.functions[site.callee], interner);
    let local_base = mir.functions[fi].locals.len() as u32;
    let drop_locals: Vec<Local> = g_locals
        .iter()
        .enumerate()
        .filter(|(i, d)| {
            !d.manual_drop
                && matches!(
                    callee_frame.kind(Local(*i as u32)),
                    Some(
                        crate::backend::shared::ValueLocalKind::Param
                            | crate::backend::shared::ValueLocalKind::Owning
                    )
                )
        })
        .map(|(i, _)| Local(local_base + i as u32))
        .collect();

    let f = &mut mir.functions[fi];
    for (i, decl) in g_locals.iter().enumerate() {
        let mut d = remap_local_decl(decl, callee_frame.kind(Local(i as u32)));
        // A borrow parameter owns nothing; once spliced in it is no longer a param, so without the
        // cursor mark later RC passes would treat it as an owned local and release it.
        if g_params.iter().any(|p| p.0 as usize == i) && !decl.is_take {
            d.is_cursor = true;
        }
        f.locals.push(d);
    }
    let block_base = f.blocks.len() as u32;
    let cont_id = BlockId(block_base + g_blocks.len() as u32);

    // Split the caller block at the call: statements before it stay; those after move to `cont`.
    let orig_term = f.blocks[site.block].terminator.clone();
    let tail: Vec<Statement> = f.blocks[site.block].stmts.split_off(site.stmt + 1);
    f.blocks[site.block].stmts.pop(); // remove the call statement itself
                                      // Bind parameters to the argument operands, applying the same numeric widening the call ABI would
                                      // (a narrower argument passed to a wider parameter), then jump into the (renumbered) callee entry.
    let params: HashSet<u32> = g_params.iter().map(|p| p.0).collect();
    // A `ref` value parameter is the caller's storage itself; binding it by assignment would
    // memcpy the value into a fresh buffer and drop the callee's writes.
    let mut aliased: Vec<Option<Local>> = vec![None; g_locals.len()];
    for (i, p) in g_params.iter().enumerate() {
        let decl = &g_locals[p.0 as usize];
        if let Operand::Copy(Place::Local(src)) = &site.args[i] {
            if decl.is_ref
                && interner.is_value_type(decl.ty)
                && mir.functions[fi].local_ty(*src) == decl.ty
            {
                aliased[p.0 as usize] = Some(*src);
            }
        }
    }
    for (i, p) in g_params.iter().enumerate() {
        if aliased[p.0 as usize].is_some() {
            continue;
        }
        let dest_local = Local(local_base + p.0);
        let pty = g_locals[p.0 as usize].ty;
        let arg = site.args[i].clone();
        let rvalue = match arg_type(&mir.functions[fi], &arg, interner) {
            Some(aty) if wasm_kind(interner, aty) != wasm_kind(interner, pty) => {
                Rvalue::Cast(arg, aty, pty)
            }
            _ => Rvalue::Use(arg),
        };
        // Re-borrow the caller after the immutable `arg_type` read above.
        mir.functions[fi].blocks[site.block]
            .stmts
            .push(Statement::Assign(Place::Local(dest_local), rvalue));
    }
    let f = &mut mir.functions[fi];
    // Zero-initialize the callee's non-parameter *reference* locals. In a standalone function these
    // start null (a fresh C frame); the callee's reference-counting relies on that — its
    // release-before-overwrite and scope-exit `Release`s assume a null baseline. Inlined into the
    // caller's frame the locals persist across executions (e.g. loop iterations), so without this
    // reset a scope-exit release on a not-yet-assigned path would free a stale pointer left by a
    // previous execution (double-free / use-after-free). Emitting the reset in the site block runs it
    // once per entry into the inlined region, matching the callee's once-at-entry zeroing.
    for (i, decl) in g_locals.iter().enumerate() {
        if !params.contains(&(i as u32)) && interner.is_rc_tracked(decl.ty) {
            f.blocks[site.block].stmts.push(Statement::Assign(
                Place::Local(Local(local_base + i as u32)),
                Rvalue::Use(Operand::Const(Const::Null)),
            ));
        }
    }
    f.blocks[site.block].terminator = Terminator::Goto(BlockId(block_base + g_entry.0));

    // Append the renumbered callee blocks, turning `Return`s into jumps to `cont`.
    // Force call-result dests Owning so the return Assign deep-copies before ValueDrop frees sources.
    for mut bb in g_blocks {
        remap_block(
            &mut bb,
            &|l: Local| aliased[l.0 as usize].unwrap_or(Local(local_base + l.0)),
            block_base,
        );
        match std::mem::replace(&mut bb.terminator, Terminator::Goto(cont_id)) {
            Terminator::Return(op) | Terminator::AsyncComplete(op) => {
                if let (Some(dest), Some(o)) = (&site.dest, op) {
                    if let Place::Local(d) = &dest {
                        let d_ty = f.local_ty(*d);
                        if interner.is_value_type(d_ty) {
                            let decl = &mut f.locals[d.0 as usize];
                            if decl.name.is_none() {
                                decl.name = Some("__vret".into());
                            }
                            decl.is_ref = false;
                        }
                    }
                    // RC insertion normalizes borrowed returns into owning locals. A call hands
                    // that +1 to its destination; a borrowed copy into a container would retain
                    // again and strand the callee's token. Nulling also prevents loop re-entry
                    // from releasing the previous invocation's returned value.
                    let (rvalue, moved) = match o {
                        Operand::Copy(Place::Local(src))
                            if interner.is_rc_tracked(f.local_ty(src)) =>
                        {
                            (Rvalue::Move { src, cast: None }, Some(src))
                        }
                        other => (Rvalue::Use(other), None),
                    };
                    bb.stmts.push(Statement::Assign(dest.clone(), rvalue));
                    if let Some(src) = moved {
                        bb.stmts.push(Statement::Assign(
                            Place::Local(src),
                            Rvalue::Use(Operand::Const(Const::Null)),
                        ));
                    }
                }
            }
            other => bb.terminator = other,
        }
        f.blocks.push(bb);
    }
    // Continuation: drop inlined owning value locals (call-site lifetime), then the caller's tail.
    let mut cont_stmts: Vec<Statement> =
        drop_locals.into_iter().map(Statement::ValueDrop).collect();
    cont_stmts.extend(tail);
    f.blocks.push(BasicBlock {
        stmts: cont_stmts,
        terminator: orig_term,
    });
}
