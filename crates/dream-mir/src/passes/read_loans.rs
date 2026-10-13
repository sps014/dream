//! References rooted in borrowed inputs survive a synchronous read-only traversal.
//! Local value copies can borrow those references even across iterator envelopes.

use crate::analysis::effects::{Effect, ModRef, ModRefTable, stmt_effects};
use crate::{Const, Local, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_hir::LayoutTable;
use dream_types::{TypeId, TypeInterner};
use std::collections::BTreeSet;

pub(super) const STAGE: &str = "read-loans";

pub(super) fn run(mir: &mut Mir, interner: &TypeInterner) -> bool {
    if mir.profile.is_debug() {
        return false;
    }
    let effects = ModRefTable::compute(mir, interner);
    let ref_calls: BTreeSet<_> = mir
        .functions
        .iter()
        .filter(|f| {
            f.params.iter().any(|p| {
                let d = &f.locals[p.0 as usize];
                d.is_ref
                    && d.name.as_deref() != Some("this")
                    && (interner.is_rc_tracked(d.ty) || interner.is_value_type(d.ty))
            })
        })
        .map(|f| (f.def, f.instance.clone()))
        .collect();
    let mut changed = false;
    for f in &mut mir.functions {
        if !f.is_async {
            changed |= function(f, interner, &mir.layouts, &effects, &ref_calls);
        }
    }
    changed
}

fn plain_value(
    ty: TypeId,
    i: &TypeInterner,
    layouts: &LayoutTable,
    seen: &mut BTreeSet<TypeId>,
) -> bool {
    if !i.is_value_type(ty) {
        return false;
    }
    if !seen.insert(ty) {
        return true;
    }
    let field = |f: &dream_hir::FieldLayout, seen: &mut BTreeSet<TypeId>| {
        !f.is_weak
            && !f.is_unowned
            && (!i.is_value_type(f.ty) || plain_value(f.ty, i, layouts, seen))
    };
    if let Some(layout) = layouts.get(ty) {
        !layout.has_destructor() && layout.fields.iter().all(|f| field(f, seen))
    } else if let Some(layout) = layouts.union(ty) {
        layout
            .variants
            .iter()
            .flat_map(|v| &v.fields)
            .all(|f| field(f, seen))
    } else {
        false
    }
}

fn function(
    f: &mut MirFunction,
    i: &TypeInterner,
    layouts: &LayoutTable,
    effects: &ModRefTable,
    ref_calls: &BTreeSet<(dream_types::DefId, Vec<TypeId>)>,
) -> bool {
    if !f
        .locals
        .iter()
        .enumerate()
        .any(|(n, d)| !d.is_ref && !f.params.contains(&Local(n as u32)) && i.is_value_type(d.ty))
    {
        return false;
    }
    let mut definitions = std::collections::BTreeMap::<Local, (usize, bool)>::new();
    for s in f.blocks.iter().flat_map(|b| &b.stmts) {
        if let Statement::Assign(Place::Local(d), rv) = s
            && !null(rv)
        {
            let def = definitions.entry(*d).or_insert((0, true));
            def.0 += 1;
            def.1 &= matches!(rv, Rvalue::New { ctor: None, .. });
        }
    }
    let fresh: BTreeSet<_> = definitions
        .into_iter()
        .filter_map(|(l, (n, new))| (n == 1 && new).then_some(l))
        .collect();
    let managed_field = |ty: TypeId, index: usize| {
        layouts
            .get(ty)
            .and_then(|l| l.fields.get(index))
            .is_none_or(|f| i.is_rc_tracked(f.ty) || i.is_value_type(f.ty))
    };
    let quiet = |m: &ModRef| match m {
        ModRef::Known(k) => {
            !k.observes_rc
                && !k.any_slot
                && k.slots.is_empty()
                && k.fields
                    .iter()
                    .all(|&(ty, field)| !i.is_value_type(ty) && !managed_field(ty, field as usize))
        }
        ModRef::Top => false,
    };
    for s in f.blocks.iter().flat_map(|b| &b.stmts) {
        let mut ok = true;
        stmt_effects(s, f, i, |e| {
            ok &= match e {
                Effect::None => true,
                Effect::Store(base, field) => {
                    (fresh.contains(&base) && !f.locals[base.0 as usize].is_ref)
                        || (!i.is_value_type(f.local_ty(base))
                            && !managed_field(f.local_ty(base), field as usize))
                }
                Effect::Call(c) => {
                    !ref_calls.contains(&(c.def, c.args.clone())) && quiet(&effects.call(c))
                }
                Effect::Ctor(d) => quiet(&effects.ctor(d)),
                Effect::Iface(..)
                | Effect::Top
                | Effect::ObserveOwnership
                | Effect::SlotStore(_)
                | Effect::GlobalStore(_) => false,
            }
        });
        if !ok {
            return false;
        }
        if matches!(s, Statement::Release(_) | Statement::ValueDrop(_)) && !quiet(effects.del()) {
            return false;
        }
    }
    if f.blocks.iter().any(|b| {
        matches!(
            b.terminator,
            Terminator::Await { .. } | Terminator::TailCall { .. }
        )
    }) {
        return false;
    }
    let mut stable: BTreeSet<Local> = (0..f.locals.len()).map(|n| Local(n as u32)).collect();
    loop {
        let rejected: Vec<_> = stable
            .iter()
            .copied()
            .filter(|&l| {
                if f.params.contains(&l) {
                    return f.locals[l.0 as usize].is_take
                        || f.locals[l.0 as usize].is_ref
                        || i.is_shared_type(f.local_ty(l))
                        || f.blocks.iter().flat_map(|b| &b.stmts).any(
                            |s| matches!(s, Statement::Assign(Place::Local(d), _) if *d == l),
                        );
                }
                let mut defined = false;
                for s in f.blocks.iter().flat_map(|b| &b.stmts) {
                    match s {
                        Statement::Assign(Place::Local(d), rv) if *d == l && !null(rv) => {
                            defined = true;
                            let good = match rv {
                                Rvalue::Use(o) => operand(o, f, i, layouts, &stable, &fresh),
                                Rvalue::Move { src, .. } => stable.contains(src),
                                Rvalue::UnionField { base, .. } => {
                                    operand(base, f, i, layouts, &stable, &fresh)
                                }
                                Rvalue::UnionNew { args, .. } => {
                                    i.is_value_type(f.local_ty(l))
                                        && args
                                            .iter()
                                            .all(|a| operand(a, f, i, layouts, &stable, &fresh))
                                }
                                Rvalue::New {
                                    ctor: None, args, ..
                                } => i.is_value_type(f.local_ty(l)) && args.is_empty(),
                                _ => false,
                            };
                            if !good {
                                return true;
                            }
                        }
                        Statement::Assign(Place::Field { base, .. }, rv)
                            if *base == l && i.is_value_type(f.local_ty(l)) =>
                        {
                            let good = match rv {
                                Rvalue::Use(o) => operand(o, f, i, layouts, &stable, &fresh),
                                Rvalue::Move { src, .. } => stable.contains(src),
                                _ => false,
                            };
                            if !good {
                                return true;
                            }
                        }
                        _ => {}
                    }
                }
                !defined
            })
            .collect();
        if rejected.is_empty() {
            break;
        }
        for l in rejected {
            stable.remove(&l);
        }
    }
    let mut views: BTreeSet<_> = stable
        .iter()
        .copied()
        .filter(|l| {
            !f.params.contains(l)
                && !f.locals[l.0 as usize].is_ref
                && plain_value(f.local_ty(*l), i, layouts, &mut BTreeSet::new())
        })
        .collect();
    let mut references: BTreeSet<_> = stable
        .iter()
        .copied()
        .filter(|l| !f.params.contains(l) && i.is_reference(f.local_ty(*l)))
        .collect();
    loop {
        let old = (views.len(), references.len());
        let rejected: Vec<_> = views
            .iter()
            .chain(&references)
            .copied()
            .filter(|&l| escapes(f, l, &views, &references))
            .collect();
        for l in rejected {
            views.remove(&l);
            references.remove(&l);
        }
        if old == (views.len(), references.len()) {
            break;
        }
    }
    if views.is_empty() {
        return false;
    }
    let mut changed = false;
    for b in &mut f.blocks {
        let old_len = b.stmts.len();
        b.stmts.retain(|s| !match s {
            Statement::ValueRetain(l) | Statement::ValueDrop(l) => views.contains(l),
            Statement::Retain(Operand::Copy(Place::Local(l)))
            | Statement::Release(Operand::Copy(Place::Local(l))) => references.contains(l),
            _ => false,
        });
        changed |= b.stmts.len() != old_len;
        for s in &mut b.stmts {
            if let Statement::Assign(Place::Field { base, .. }, rv) = s
                && views.contains(base)
                && let Rvalue::Move { src, cast: None } = rv
                && references.contains(src)
            {
                *rv = Rvalue::Use(Operand::Copy(Place::Local(*src)));
            }
        }
    }
    for l in views {
        let d = &mut f.locals[l.0 as usize];
        changed |= !d.borrows_refs;
        d.borrows_refs = true;
        d.manual_drop = true;
    }
    for l in references {
        f.locals[l.0 as usize].is_cursor = true;
    }
    changed
}

fn null(rv: &Rvalue) -> bool {
    matches!(rv, Rvalue::Use(Operand::Const(Const::Null)))
}

fn operand(
    o: &Operand,
    f: &MirFunction,
    i: &TypeInterner,
    layouts: &LayoutTable,
    stable: &BTreeSet<Local>,
    fresh: &BTreeSet<Local>,
) -> bool {
    match o {
        Operand::Const(_) => true,
        Operand::Copy(Place::Local(l)) => {
            stable.contains(l)
                || !(i.is_rc_tracked(f.local_ty(*l)) || i.is_value_type(f.local_ty(*l)))
        }
        Operand::Copy(Place::Index { base, .. }) => stable.contains(base),
        Operand::Copy(Place::Field { base, field }) => {
            let Some(layout) = layouts
                .get(f.local_ty(*base))
                .and_then(|l| l.fields.get(*field))
            else {
                return false;
            };
            if layout.is_weak || layout.is_unowned || i.is_shared_type(f.local_ty(*base)) {
                return false;
            }
            if stable.contains(base) {
                return true;
            }
            if !fresh.contains(base) {
                return false;
            }
            let mut any = false;
            for s in f.blocks.iter().flat_map(|b| &b.stmts) {
                if let Statement::Assign(Place::Field { base: b, field: n }, rv) = s
                    && b == base
                    && n == field
                {
                    any = true;
                    if !match rv {
                        Rvalue::Use(Operand::Copy(Place::Local(l)))
                        | Rvalue::Move { src: l, .. } => stable.contains(l),
                        Rvalue::Use(Operand::Const(_)) => true,
                        _ => false,
                    } {
                        return false;
                    }
                }
            }
            any
        }
        _ => false,
    }
}

fn escapes(f: &MirFunction, l: Local, views: &BTreeSet<Local>, refs: &BTreeSet<Local>) -> bool {
    let member = |d: &Local| views.contains(d) || refs.contains(d);
    let op_is = |o: &Operand| matches!(o, Operand::Copy(Place::Local(d)) if *d == l);
    for b in &f.blocks {
        if matches!(&b.terminator, Terminator::Return(Some(o)) | Terminator::AsyncComplete(Some(o)) if op_is(o))
        {
            return true;
        }
        for s in &b.stmts {
            match s {
                Statement::Assign(p, Rvalue::Use(o)) if op_is(o) => {
                    if !matches!(p, Place::Local(d) if member(d))
                        && !matches!(p, Place::Field { base, .. } if views.contains(base))
                    {
                        return true;
                    }
                }
                Statement::Assign(p, Rvalue::Move { src, .. }) if *src == l => {
                    if !matches!(p, Place::Local(d) if member(d))
                        && !matches!(p, Place::Field { base, .. } if views.contains(base))
                    {
                        return true;
                    }
                }
                Statement::Call { callee, args }
                | Statement::Assign(_, Rvalue::Call { callee, args }) => {
                    if args.iter().enumerate().any(|(n, o)| {
                        op_is(o) && callee.take_params.get(n).copied().unwrap_or(false)
                    }) {
                        return true;
                    }
                }
                Statement::Assign(Place::Local(d), Rvalue::UnionNew { args, .. })
                    if args.iter().any(op_is) =>
                {
                    if !views.contains(d) {
                        return true;
                    }
                }
                Statement::Assign(
                    _,
                    Rvalue::ArrayLit { elems: args, .. }
                    | Rvalue::Tuple { elems: args, .. }
                    | Rvalue::New { args, .. },
                ) if args.iter().any(op_is) => return true,
                Statement::Assign(_, Rvalue::Cast(o, _, _)) | Statement::ForceFree(o)
                    if op_is(o) =>
                {
                    return true;
                }
                _ => {}
            }
        }
    }
    false
}

#[cfg(test)]
mod tests;
