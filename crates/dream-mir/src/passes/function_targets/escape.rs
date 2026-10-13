//! Opaque access to a box's representation invalidates every target of that function shape.

use super::{IntrinsicOp, Key, Mir, Place, Rvalue, Statement, TypeId, TypeInterner, operand_type};
use dream_types::TyKind;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn box_types(mir: &Mir, interner: &TypeInterner) -> BTreeSet<TypeId> {
    let all: BTreeSet<_> = mir
        .functions
        .iter()
        .chain(&mir.polls)
        .flat_map(|f| &f.locals)
        .map(|l| l.ty)
        .filter(|ty| matches!(interner.kind(*ty), TyKind::Func(..)))
        .collect();
    let bodies: BTreeSet<Key> = mir
        .functions
        .iter()
        .map(|f| (f.def, f.instance.clone()))
        .collect();
    let intrinsics: BTreeMap<_, _> = mir
        .intrinsics
        .iter()
        .filter_map(|(d, key)| IntrinsicOp::from_key(key).map(|op| (*d, op)))
        .collect();
    let mut escaped = BTreeSet::new();
    let mut foreign = false;
    for f in mir.functions.iter().chain(&mir.polls) {
        for stmt in f.blocks.iter().flat_map(|b| &b.stmts) {
            match stmt {
                Statement::Assign(Place::Deref { .. }, _) | Statement::ForceFree(_) => return all,
                Statement::Assign(_, Rvalue::Cast(_, from, to)) if from != to => {
                    collect(*from, mir, interner, &all, true, &mut escaped);
                }
                Statement::Call { callee, args }
                | Statement::Assign(_, Rvalue::Call { callee, args })
                    if !bodies.contains(&(callee.def, callee.args.clone())) =>
                {
                    if let Some(op) = intrinsics.get(&callee.def) {
                        if matches!(op, IntrinsicOp::FuncboxEnv | IntrinsicOp::FuncboxFuncidx) {
                            continue;
                        }
                        for arg in args {
                            if let Some(ty) = operand_type(f, arg)
                                && matches!(interner.kind(ty), TyKind::Func(..))
                            {
                                escaped.insert(ty);
                            }
                        }
                    } else {
                        foreign = true;
                        for arg in args {
                            if let Some(ty) = operand_type(f, arg) {
                                collect(ty, mir, interner, &all, true, &mut escaped);
                            }
                        }
                    }
                }
                Statement::JsCall { args, .. }
                | Statement::Assign(_, Rvalue::JsCall { args, .. }) => {
                    foreign = true;
                    for (_, ty) in args {
                        collect(*ty, mir, interner, &all, true, &mut escaped);
                    }
                }
                Statement::Assign(
                    _,
                    Rvalue::New { args, .. }
                    | Rvalue::UnionNew { args, .. }
                    | Rvalue::ArrayLit { elems: args, .. }
                    | Rvalue::Tuple { elems: args, .. },
                ) => {
                    for arg in args {
                        if let Some(ty) = operand_type(f, arg)
                            && matches!(interner.kind(ty), TyKind::Func(..))
                        {
                            escaped.insert(ty);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if foreign {
        // Async frames have hidden storage edges. Their local types account for captures that
        // a foreign Future consumer could otherwise reach through an opaque frame pointer.
        for f in mir
            .functions
            .iter()
            .chain(&mir.polls)
            .filter(|f| f.is_async || f.hir_fn.as_ref().is_some_and(|h| h.is_async))
        {
            for local in &f.locals {
                collect(local.ty, mir, interner, &all, false, &mut escaped);
            }
        }
    }
    escaped
}

fn collect(
    root: TypeId,
    mir: &Mir,
    interner: &TypeInterner,
    all: &BTreeSet<TypeId>,
    unknown_top: bool,
    out: &mut BTreeSet<TypeId>,
) {
    let mut seen = BTreeSet::new();
    let mut work = vec![root];
    while let Some(ty) = work.pop() {
        if !seen.insert(ty) {
            continue;
        }
        match interner.kind(ty) {
            TyKind::Func(..) => {
                out.insert(ty);
            }
            TyKind::Array(elem) => work.push(*elem),
            TyKind::Tuple(fields) => work.extend(fields),
            TyKind::Struct(..) => {
                if let Some(layout) = mir.layouts.get(ty) {
                    work.extend(layout.fields.iter().map(|f| f.ty));
                } else if unknown_top {
                    out.extend(all);
                }
            }
            TyKind::Union(..) => {
                if let Some(layout) = mir.layouts.union(ty) {
                    work.extend(layout.variants.iter().flat_map(|v| &v.fields).map(|f| f.ty));
                } else if unknown_top {
                    out.extend(all);
                }
            }
            TyKind::Object | TyKind::Interface(..) | TyKind::Js if unknown_top => {
                out.extend(all);
            }
            _ => {}
        }
    }
}

pub(super) fn result_type(rv: &Rvalue, f: &super::MirFunction) -> Option<TypeId> {
    match rv {
        Rvalue::Call { callee, .. } => Some(callee.ret),
        Rvalue::Use(o) => operand_type(f, o),
        Rvalue::Cast(_, _, to) => Some(*to),
        _ => None,
    }
}
