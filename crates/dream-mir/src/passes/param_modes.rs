//! Scalar-array kernels cannot release an argument's owner through a callback, global store or
//! managed element. Their closed call graph can therefore borrow arrays, including on recursive
//! edges, without moving an observable destructor or invalidating a caller-held reference.

use crate::{Callee, Mir, MirFunction, Place, Rvalue, Statement, Terminator};
use dream_types::{TyKind, TypeInterner};
use indexmap::{IndexMap, IndexSet};

pub(crate) const STAGE: &str = "param-modes";
type Key = (dream_types::DefId, Vec<dream_types::TypeId>);

fn key(callee: &Callee) -> Key {
    (callee.def, callee.args.clone())
}

fn scalar(ty: dream_types::TypeId, interner: &TypeInterner) -> bool {
    matches!(interner.kind(ty), TyKind::Void)
        || (matches!(interner.kind(ty), TyKind::Prim(_)) && !interner.is_rc_tracked(ty))
}

fn scalar_array(ty: dream_types::TypeId, interner: &TypeInterner) -> bool {
    matches!(interner.kind(ty), TyKind::Array(elem) if scalar(*elem, interner))
}

fn kernel(f: &MirFunction, interner: &TypeInterner) -> bool {
    if f.is_async
        || !scalar(f.ret, interner)
        || f.locals
            .iter()
            .any(|d| d.is_ref || !(scalar(d.ty, interner) || scalar_array(d.ty, interner)))
    {
        return false;
    }
    f.blocks.iter().all(|block| {
        block.stmts.iter().all(|stmt| match stmt {
            Statement::Assign(place, rv) => {
                matches!(place, Place::Local(_) | Place::Index { .. })
                    && matches!(
                        rv,
                        Rvalue::Use(_)
                            | Rvalue::Binary(..)
                            | Rvalue::CheckedBinary(..)
                            | Rvalue::Unary(..)
                            | Rvalue::CheckedNeg(_)
                            | Rvalue::Cast(..)
                            | Rvalue::ArrayLen(_)
                            | Rvalue::Select { .. }
                            | Rvalue::Call { .. }
                    )
            }
            Statement::Call { .. } | Statement::Nop | Statement::SourceLine(_) => true,
            _ => false,
        }) && matches!(
            block.terminator,
            Terminator::Goto(_)
                | Terminator::If { .. }
                | Terminator::Switch { .. }
                | Terminator::Return(_)
                | Terminator::Unreachable
        )
    })
}

fn callees(f: &MirFunction) -> impl Iterator<Item = &Callee> {
    f.blocks
        .iter()
        .flat_map(|b| &b.stmts)
        .filter_map(|s| match s {
            Statement::Call { callee, .. } | Statement::Assign(_, Rvalue::Call { callee, .. }) => {
                Some(callee)
            }
            _ => None,
        })
}

pub(crate) fn run(mir: &mut Mir, interner: &TypeInterner) -> bool {
    if mir.profile.is_debug() {
        return false;
    }
    // Starting optimistically admits a recursive component; removing unsafe callees propagates
    // back to every caller before any ownership mode is changed.
    let mut candidates: IndexSet<Key> = mir
        .functions
        .iter()
        .filter(|f| kernel(f, interner) && !mir.exports.iter().any(|(def, _)| *def == f.def))
        .map(|f| (f.def, f.instance.clone()))
        .collect();
    loop {
        let rejected: Vec<_> = mir
            .functions
            .iter()
            .filter(|f| {
                candidates.contains(&(f.def, f.instance.clone()))
                    && callees(f).any(|c| !candidates.contains(&key(c)))
            })
            .map(|f| (f.def, f.instance.clone()))
            .collect();
        if rejected.is_empty() {
            break;
        }
        for k in rejected {
            candidates.shift_remove(&k);
        }
    }
    let mut modes: IndexMap<Key, Vec<bool>> = IndexMap::new();
    let mut changed = false;
    for f in &mut mir.functions {
        if !candidates.contains(&(f.def, f.instance.clone())) {
            continue;
        }
        for p in &f.params {
            let d = &mut f.locals[p.0 as usize];
            if d.is_take && scalar_array(d.ty, interner) {
                d.is_take = false;
                changed = true;
            }
        }
        modes.insert(
            (f.def, f.instance.clone()),
            f.params
                .iter()
                .map(|p| f.locals[p.0 as usize].is_take)
                .collect(),
        );
    }
    for f in mir.functions.iter_mut().chain(&mut mir.polls) {
        for stmt in f.blocks.iter_mut().flat_map(|b| &mut b.stmts) {
            let callee = match stmt {
                Statement::Call { callee, .. }
                | Statement::Assign(_, Rvalue::Call { callee, .. }) => callee,
                _ => continue,
            };
            if let Some(takes) = modes.get(&key(callee)) {
                changed |= callee.take_params != *takes;
                callee.take_params.clone_from(takes);
            }
        }
    }
    changed
}
