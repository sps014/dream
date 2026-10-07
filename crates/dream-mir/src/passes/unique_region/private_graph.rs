//! Private-graph proof for cycle-capable allocations inside region-safe functions.
//!
//! A region object is reclaimed in bulk, so a cycle-capable class may only enter the region when
//! the collector never needs to see it: it gets the untracked descriptor (`AllocPolicy::Private`).
//! That is sound only if the object can reach nothing but other objects born in the same region
//! and nothing outside can reach it. `region_safe_body` already rules out escape through globals,
//! containers, closures, interface/indirect calls, async and destructors; this adds the inward
//! edge rule: every reference stored into a private object is fresh (allocated by this region),
//! and the class has no weak or unowned fields, whose registry entries would outlive the region.

use super::*;

pub(super) fn private_graph_ok(cx: &mut SafeCx<'_>, f: &MirFunction) -> bool {
    let sites: Vec<(Local, TypeId, &[Operand])> = f
        .blocks
        .iter()
        .flat_map(|b| &b.stmts)
        .filter_map(|s| match s {
            Statement::Assign(Place::Local(d), Rvalue::New { ty, args, .. })
                if crate::ownership::cycle_capable(&cx.mir.layouts, cx.interner, *ty) =>
            {
                Some((*d, *ty, args.as_slice()))
            }
            _ => None,
        })
        .collect();
    let this_stores_params = cx.ctor_only.contains(&f.def) && constructs_cycle_capable(cx, f);
    if sites.is_empty() && !this_stores_params && !stores_into_cycle_capable(cx, f) {
        return true;
    }
    for &(_, ty, _) in &sites {
        let Some(layout) = cx.mir.layouts.get(ty) else {
            return false;
        };
        if layout.fields.iter().any(|fl| fl.is_weak || fl.is_unowned) {
            return false;
        }
    }
    let fresh = fresh_locals(cx, f);
    let this_local = f.params.first().copied();
    let params: BTreeSet<u32> = f.params.iter().skip(1).map(|p| p.0).collect();
    let stored_ok = |op: &Operand, this_base: bool| match op {
        Operand::Const(_) => true,
        Operand::Copy(Place::Local(l)) => {
            !cx.interner.is_rc_tracked(f.local_ty(*l))
                || fresh.contains(&l.0)
                || (this_base && this_stores_params && params.contains(&l.0))
        }
        Operand::Copy(_) => false,
    };
    for &(_, _, args) in &sites {
        if !args.iter().all(|a| stored_ok(a, false)) {
            return false;
        }
    }
    for s in f.blocks.iter().flat_map(|b| &b.stmts) {
        // A delegated constructor stores its arguments into the same `this`.
        if let Statement::Assign(_, Rvalue::Call { callee, args }) = s
            && cx.ctor_only.contains(&callee.def)
            && !args.iter().skip(1).all(|a| stored_ok(a, true))
        {
            return false;
        }
        let Statement::Assign(Place::Field { base, .. }, rv) = s else {
            continue;
        };
        let this_base = this_local == Some(*base);
        let ok = match rv {
            Rvalue::Use(op) | Rvalue::Cast(op, _, _) => stored_ok(op, this_base),
            Rvalue::Move { src, .. } => stored_ok(&Operand::Copy(Place::Local(*src)), this_base),
            Rvalue::New { .. } | Rvalue::Call { .. } => true,
            Rvalue::Tuple { .. } | Rvalue::UnionNew { .. } | Rvalue::Select { .. } => false,
            _ => true,
        };
        if !ok {
            return false;
        }
    }
    true
}

/// Rewrite every cycle-capable `New` in a region-safe function to `AllocPolicy::Private`. The
/// policy only matters while a region is active; outside one the runtime tracks it as usual.
pub(super) fn mark_private(
    mir: &mut Mir,
    interner: &TypeInterner,
    safe: &IndexMap<(DefId, Vec<TypeId>), bool>,
) {
    let layouts = &mir.layouts;
    for f in &mut mir.functions {
        if !safe
            .get(&(f.def, f.instance.clone()))
            .copied()
            .unwrap_or(false)
        {
            continue;
        }
        for s in f.blocks.iter_mut().flat_map(|b| &mut b.stmts) {
            if let Statement::Assign(_, Rvalue::New { ty, policy, .. }) = s
                && crate::ownership::cycle_capable(layouts, interner, *ty)
            {
                *policy = crate::AllocPolicy::Private;
            }
        }
    }
}

fn constructs_cycle_capable(cx: &SafeCx<'_>, f: &MirFunction) -> bool {
    f.params
        .first()
        .is_some_and(|this| crate::ownership::cycle_capable(&cx.mir.layouts, cx.interner, f.local_ty(*this)))
}

fn stores_into_cycle_capable(cx: &SafeCx<'_>, f: &MirFunction) -> bool {
    f.blocks.iter().flat_map(|b| &b.stmts).any(|s| {
        matches!(s, Statement::Assign(Place::Field { base, .. }, _)
            if crate::ownership::cycle_capable(&cx.mir.layouts, cx.interner, f.local_ty(*base)))
    })
}

/// Must-analysis: a local is fresh only if every definition allocates (directly or through a
/// region-safe callee, whose result `region_safe_body` already proved fresh), is null, or copies
/// a fresh local. Parameters and `await` results never are.
fn fresh_locals(cx: &mut SafeCx<'_>, f: &MirFunction) -> BTreeSet<u32> {
    let mut candidate: BTreeSet<u32> = BTreeSet::new();
    let mut copies: Vec<(u32, u32)> = Vec::new();
    let mut rejected: BTreeSet<u32> = f.params.iter().map(|p| p.0).collect();
    for b in &f.blocks {
        if let Terminator::Await { dest: Some(d), .. } = &b.terminator {
            rejected.insert(d.0);
        }
        for s in &b.stmts {
            let Statement::Assign(Place::Local(d), rv) = s else {
                continue;
            };
            candidate.insert(d.0);
            match rv {
                Rvalue::New { .. } | Rvalue::Use(Operand::Const(Const::Null)) => {}
                Rvalue::Call { callee, .. } if callee_safe(cx, callee) => {}
                Rvalue::Use(Operand::Copy(Place::Local(s)))
                | Rvalue::Cast(Operand::Copy(Place::Local(s)), _, _)
                | Rvalue::Move { src: s, .. } => copies.push((d.0, s.0)),
                _ => {
                    rejected.insert(d.0);
                }
            }
        }
    }
    let mut fresh: BTreeSet<u32> = candidate.difference(&rejected).copied().collect();
    let mut changed = true;
    while changed {
        changed = false;
        for (d, src) in &copies {
            if fresh.contains(d) && !fresh.contains(src) {
                fresh.remove(d);
                changed = true;
            }
        }
    }
    fresh
}
