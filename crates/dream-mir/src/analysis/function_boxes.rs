//! Shared closed-world function-box representation and capture facts.

use crate::{Const, Mir, MirFunction, Operand, Place, Rvalue, Statement};
use dream_abi::intrinsics::IntrinsicOp;
use dream_types::{DefId, TyKind, TypeId, TypeInterner};
use std::collections::{BTreeMap, BTreeSet};

type Key = (DefId, Vec<TypeId>);

pub(crate) fn box_types(mir: &Mir, interner: &TypeInterner) -> BTreeSet<TypeId> {
    let all: BTreeSet<_> = mir
        .functions
        .iter()
        .chain(&mir.polls)
        .flat_map(|f| &f.locals)
        .map(|l| l.ty)
        .filter(|ty| matches!(interner.kind(*ty), TyKind::Func(..)))
        .collect();
    let bodies: BTreeMap<Key, _> = mir
        .functions
        .iter()
        .map(|f| ((f.def, f.instance.clone()), f))
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
                Statement::Assign(Place::Deref { .. }, _)
                | Statement::RawStore { .. }
                | Statement::ForceFree(_) => return all,
                Statement::Assign(_, Rvalue::Cast(_, from, to)) if from != to => {
                    collect(*from, mir, interner, &all, true, &mut escaped);
                }
                Statement::Call { callee, args }
                | Statement::Assign(_, Rvalue::Call { callee, args })
                    if !bodies.contains_key(&(callee.def, callee.args.clone())) =>
                {
                    if let Some(op) = intrinsics.get(&callee.def) {
                        if matches!(
                            op,
                            IntrinsicOp::FuncboxNew
                                | IntrinsicOp::FuncboxEnv
                                | IntrinsicOp::FuncboxFuncidx
                        ) {
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
                        // An opaque result cannot expose a typed box until a cast does so.
                        // Known result layouts can carry boxes directly and must invalidate them.
                        collect(callee.ret, mir, interner, &all, false, &mut escaped);
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
                Statement::Call { callee, args }
                | Statement::Assign(_, Rvalue::Call { callee, args }) => {
                    if let Some(body) = bodies.get(&(callee.def, callee.args.clone())) {
                        for (param, arg) in body.params.iter().zip(args) {
                            if body.locals[param.0 as usize].is_ref
                                && let Some(ty) = operand_type(f, arg)
                            {
                                collect(ty, mir, interner, &all, false, &mut escaped);
                            }
                        }
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

pub(crate) fn result_type(rv: &Rvalue, f: &MirFunction) -> Option<TypeId> {
    match rv {
        Rvalue::Call { callee, .. } => Some(callee.ret),
        Rvalue::Use(o) => operand_type(f, o),
        Rvalue::Cast(_, _, to) => Some(*to),
        _ => None,
    }
}

pub(crate) fn operand_type(f: &MirFunction, o: &Operand) -> Option<TypeId> {
    match o {
        Operand::Copy(Place::Local(l)) => Some(f.local_ty(*l)),
        _ => None,
    }
}

pub(crate) fn capture_free_types(mir: &Mir, interner: &TypeInterner) -> BTreeSet<TypeId> {
    if !mir.exports.is_empty() {
        return BTreeSet::new();
    }
    let mut rejected = box_types(mir, interner);
    let mut created = BTreeSet::new();
    let bodies: BTreeSet<_> = mir
        .functions
        .iter()
        .map(|f| (f.def, f.instance.clone()))
        .collect();
    let intrinsics: BTreeMap<_, _> = mir
        .intrinsics
        .iter()
        .filter_map(|(def, key)| IntrinsicOp::from_key(key).map(|op| (*def, op)))
        .collect();
    for f in mir.functions.iter().chain(&mir.polls) {
        for stmt in f.blocks.iter().flat_map(|b| &b.stmts) {
            let Statement::Assign(Place::Local(dest), rv) = stmt else {
                continue;
            };
            let ty = f.local_ty(*dest);
            if !matches!(interner.kind(ty), TyKind::Func(..)) {
                continue;
            }
            match rv {
                Rvalue::Call { callee, args }
                    if intrinsics.get(&callee.def) == Some(&IntrinsicOp::FuncboxNew) =>
                {
                    if matches!(
                        args.get(1),
                        Some(Operand::Const(Const::Int(0) | Const::Null))
                    ) {
                        created.insert(ty);
                    } else {
                        rejected.insert(ty);
                    }
                }
                Rvalue::Use(Operand::Copy(
                    Place::Local(_) | Place::Field { .. } | Place::Index { .. } | Place::Global(_),
                ))
                | Rvalue::Use(Operand::Const(Const::Null))
                | Rvalue::Select { .. }
                | Rvalue::UnionField { .. } => {}
                Rvalue::Call { callee, .. }
                    if bodies.contains(&(callee.def, callee.args.clone())) => {}
                Rvalue::Cast(_, from, to) if from == to => {}
                _ => {
                    rejected.insert(ty);
                }
            }
        }
    }
    created.retain(|ty| !rejected.contains(ty));
    created
}
