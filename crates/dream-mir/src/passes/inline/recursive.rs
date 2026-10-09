//! Expand one level of small scalar recursion once, before the ordinary DAG inliner. Newly
//! introduced calls are never reconsidered here, so recursive code growth has a fixed bound.

use super::eligibility::Site;
use super::splice::perform_inline;
use crate::{Mir, MirFunction, Place, Rvalue, Statement, Terminator};
use dream_types::{TyKind, TypeInterner};

fn sites(f: &MirFunction, fi: usize) -> Vec<Site> {
    let mut out = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for (si, s) in b.stmts.iter().enumerate() {
            if let Statement::Assign(dest, Rvalue::Call { callee, args }) = s
                && callee.def == f.def
                && callee.args == f.instance
            {
                out.push(Site {
                    block: bi,
                    stmt: si,
                    callee: fi,
                    args: args.clone(),
                    dest: Some(dest.clone()),
                });
            }
        }
    }
    out
}

fn eligible(f: &MirFunction, interner: &TypeInterner) -> bool {
    !f.is_async
        && f.inline != dream_hir::InlineHint::Never
        && matches!(interner.kind(f.ret), TyKind::Prim(_))
        && !interner.is_rc_tracked(f.ret)
        && f.blocks.iter().all(|b| {
            matches!(
                b.terminator,
                Terminator::Goto(_)
                    | Terminator::If { .. }
                    | Terminator::Switch { .. }
                    | Terminator::Return(_)
                    | Terminator::Unreachable
            )
        })
        && f.blocks.len() <= 12
        && f.blocks.iter().map(|b| b.stmts.len()).sum::<usize>() <= 32
        && f.locals.iter().all(|d| {
            !d.is_ref
                && matches!(interner.kind(d.ty), TyKind::Prim(_))
                && !interner.is_rc_tracked(d.ty)
        })
        && f.blocks.iter().flat_map(|b| &b.stmts).all(|s| match s {
            Statement::Assign(Place::Local(_), rv) => match rv {
                Rvalue::Use(_)
                | Rvalue::Binary(..)
                | Rvalue::CheckedBinary(..)
                | Rvalue::Unary(..)
                | Rvalue::CheckedNeg(_)
                | Rvalue::Cast(..)
                | Rvalue::Select { .. } => true,
                Rvalue::Call { callee, .. } => callee.def == f.def && callee.args == f.instance,
                _ => false,
            },
            Statement::Nop | Statement::SourceLine(_) => true,
            _ => false,
        })
}

pub(crate) fn run(mir: &mut Mir, interner: &TypeInterner) -> bool {
    if mir.profile.is_debug() {
        return false;
    }
    let mut changed = false;
    let n = mir.functions.len();
    for fi in 0..n {
        let f = &mir.functions[fi];
        if !eligible(f, interner) || mir.exports.iter().any(|(def, _)| *def == f.def) {
            continue;
        }
        let calls = sites(f, fi);
        // A single recursive edge is already a candidate for tail recursion elimination.
        if calls.len() != 2 {
            continue;
        }
        let snapshot = MirFunction {
            def: f.def,
            symbol: f.symbol.clone(),
            instance: f.instance.clone(),
            name: f.name.clone(),
            params: f.params.clone(),
            ret: f.ret,
            locals: f.locals.clone(),
            blocks: f.blocks.clone(),
            entry: f.entry,
            is_async: false,
            hir_fn: None,
            file: f.file.clone(),
            inline: f.inline,
        };
        mir.functions.push(snapshot);
        // Splicing splits the block after a call; reverse order keeps earlier original indices valid.
        for mut site in calls.into_iter().rev() {
            site.callee = n;
            perform_inline(mir, fi, site, interner);
        }
        mir.functions.pop();
        changed = true;
    }
    changed
}
