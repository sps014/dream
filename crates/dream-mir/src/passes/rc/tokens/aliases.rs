use crate::Local;
use crate::MirFunction;
use crate::Operand;
use crate::Place;
use crate::Rvalue;
use crate::Statement;
use dream_types::TyKind;
use dream_types::TypeInterner;
use indexmap::IndexMap;
use indexmap::IndexSet;
use std::collections::BTreeSet;

/// Copy / niche-union / field / index / `unwrap_or`: dest aliases `src`.
///
/// `calls`: also `dest = f(src, …)` so leftover_order releases a `JsonValue.get` dest before
/// leftover of `this`. Do not use that edge for leftover_waits: `r.text()` / concat dests would
/// wait for a still-live receiver and leak.
pub(crate) fn leftover_alias_parent(
    func: &MirFunction,
    interner: &TypeInterner,
    calls: bool,
) -> IndexMap<u32, u32> {
    let mut parent = IndexMap::new();
    let ty = |l: u32| func.locals.get(l as usize).map(|d| d.ty);
    for block in &func.blocks {
        for stmt in &block.stmts {
            match stmt {
                Statement::Assign(
                    Place::Local(dest),
                    Rvalue::Use(Operand::Copy(Place::Local(src))),
                ) if calls || ty(dest.0) == ty(src.0) => {
                    parent.entry(dest.0).or_insert(src.0);
                }
                Statement::Assign(
                    Place::Local(dest),
                    Rvalue::Cast(Operand::Copy(Place::Local(src)), _, _),
                ) => {
                    // `funcbox_new(idx, env as int)`: leftover the box before leftover of the
                    // env array so typed `release_array_*` still sees the last +1.
                    parent.entry(dest.0).or_insert(src.0);
                }
                Statement::Assign(
                    Place::Local(dest),
                    Rvalue::Use(Operand::Copy(Place::Field { base, .. })),
                ) => {
                    parent.entry(dest.0).or_insert(base.0);
                }
                Statement::Assign(
                    Place::Local(dest),
                    Rvalue::Use(Operand::Copy(Place::Index { base, .. })),
                ) => {
                    parent.entry(dest.0).or_insert(base.0);
                }
                Statement::Assign(
                    Place::Local(dest),
                    Rvalue::UnionField {
                        base: Operand::Copy(Place::Local(src)),
                        ..
                    },
                ) => {
                    parent.entry(dest.0).or_insert(src.0);
                }
                Statement::Assign(Place::Local(dest), Rvalue::Call { callee, args, .. })
                    if !args.is_empty() =>
                {
                    let Operand::Copy(Place::Local(src)) = &args[0] else {
                        continue;
                    };
                    if dest.0 != src.0 {
                        // Borrow `this` methods (`JsonValue.get`): leftover dest waits for `this`
                        // and leftover_order releases dest first. Take first-arg (`concat`) must
                        // not wait — the dest is a new object and the first arg may stay live.
                        let borrow_this = callee.take_params.first() != Some(&true);
                        if calls || borrow_this {
                            parent.entry(dest.0).or_insert(src.0);
                        }
                    }
                    if args.len() == 2 {
                        let Operand::Copy(Place::Local(fb)) = &args[1] else {
                            continue;
                        };
                        if ty(dest.0) == ty(fb.0) {
                            parent.entry(dest.0).or_insert(src.0);
                        }
                    }
                    // `funcbox_new(idx, env)`: dest is a fun, arg0 is int. Leftover the box before
                    // leftover of the env array so typed array release still sees the last +1.
                    if calls && args.len() >= 2 && ty(dest.0) != ty(src.0)
                        && let Operand::Copy(Place::Local(env)) = &args[1] {
                            parent.insert(dest.0, env.0);
                        }
                    // `Next(handler)`: dest is the cell, last arg is the funcbox. Leftover of
                    // `Next` must count as covering that box so env leftover still 2→1 then last-drop.
                    if calls
                        && let Some(Operand::Copy(Place::Local(last))) = args.last()
                            && matches!(interner.kind(func.local_ty(*last)), TyKind::Func(..)) {
                                parent.insert(dest.0, last.0);
                            }
                }
                Statement::Assign(Place::Local(dest), Rvalue::New { args, .. }) if calls => {
                    for arg in args.iter().rev() {
                        let Operand::Copy(Place::Local(src)) = arg else {
                            continue;
                        };
                        if dest.0 != src.0 {
                            parent.entry(dest.0).or_insert(src.0);
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    parent
}

pub(super) fn leftover_waits_for_live_parent(
    parent: &IndexMap<u32, u32>,
    local: u32,
    live: &IndexSet<u32>,
) -> bool {
    let mut x = local;
    let mut seen = IndexSet::new();
    while seen.insert(x) {
        let Some(&p) = parent.get(&x) else {
            return false;
        };
        if live.contains(&p) && p != local {
            return true;
        }
        x = p;
    }
    false
}

/// Locals in `ids` that should `Release`. Dest leftover is delayed until the parent leftover
/// site (`order_parent` skip in `transfer_block`) so extras and `this` share one batch;
/// skip-coalesce here last-refs a map occupant (`union_json`) or leaks extras (`json_parse`).
pub(crate) fn leftover_keep(
    _func: &MirFunction,
    ids: impl IntoIterator<Item = u32>,
) -> IndexSet<u32> {
    ids.into_iter().collect()
}

/// `(funcbox dest, env RC root)` for each `funcbox_new` (env may be an `int` pun of an `object[]`).
pub(crate) fn funcbox_env_pairs(func: &MirFunction, interner: &TypeInterner) -> Vec<(u32, u32)> {
    let parent = leftover_alias_parent(func, interner, true);
    let mut pairs = Vec::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            let (dest, args) = match stmt {
                Statement::Assign(Place::Local(dest), Rvalue::Call { args, .. }) => (dest, args),
                _ => continue,
            };
            if args.len() < 2 {
                continue;
            }
            if !matches!(interner.kind(func.local_ty(*dest)), TyKind::Func(..)) {
                continue;
            }
            let Operand::Copy(Place::Local(env)) = &args[1] else {
                continue;
            };
            let mut x = env.0;
            let mut seen = IndexSet::new();
            loop {
                if !seen.insert(x) {
                    break;
                }
                if interner.is_rc_tracked(func.local_ty(Local(x))) {
                    pairs.push((dest.0, x));
                    break;
                }
                match parent.get(&x) {
                    Some(&p) => x = p,
                    None => break,
                }
            }
        }
    }
    pairs
}

pub(crate) fn funcbox_env_rc_roots(func: &MirFunction, interner: &TypeInterner) -> IndexSet<u32> {
    funcbox_env_pairs(func, interner)
        .into_iter()
        .map(|(_, env)| env)
        .collect()
}

/// Child alias dests before parents so leftover Release of an extra-retain occupant runs while
/// the container still holds +1 (parent-first last-refs the map slot under the dest).
pub(crate) fn leftover_order(
    parent: &IndexMap<u32, u32>,
    ids: impl IntoIterator<Item = u32>,
    defer: &IndexSet<u32>,
) -> Vec<u32> {
    let ids: Vec<u32> = ids.into_iter().collect();
    let set: IndexSet<u32> = ids.iter().copied().collect();
    let mut indeg: IndexMap<u32, u32> = ids.iter().map(|&x| (x, 0)).collect();
    let mut edge: IndexMap<u32, u32> = IndexMap::new();
    for &d in &ids {
        let mut x = d;
        let mut seen = IndexSet::new();
        while seen.insert(x) {
            let Some(&p) = parent.get(&x) else {
                break;
            };
            if set.contains(&p) {
                edge.insert(d, p);
                if let Some(n) = indeg.get_mut(&p) {
                    *n += 1;
                }
                break;
            }
            x = p;
        }
    }
    let mut ready: BTreeSet<(u8, u32)> = indeg
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(&x, _)| {
            // Children (have a parent) before roots so leftover of `this` (often local 0)
            // cannot last-ref a map under still-live get/unwrap dests.
            let child = if parent.contains_key(&x) { 0 } else { 1 };
            (child, x)
        })
        .collect();
    let mut out = Vec::with_capacity(ids.len());
    let mut left = set;
    while let Some((_, d)) = ready.pop_first() {
        if !left.swap_remove(&d) {
            continue;
        }
        out.push(d);
        if let Some(&p) = edge.get(&d)
            && let Some(n) = indeg.get_mut(&p) {
                *n = n.saturating_sub(1);
                if *n == 0 && left.contains(&p) {
                    let child = if parent.contains_key(&p) { 0 } else { 1 };
                    ready.insert((child, p));
                }
            }
    }
    let mut rest: Vec<u32> = left.into_iter().collect();
    rest.sort_unstable();
    out.extend(rest);
    if defer.is_empty() {
        return out;
    }
    let mut first = Vec::with_capacity(out.len());
    let mut last = Vec::new();
    for x in out {
        if defer.contains(&x) {
            last.push(x);
        } else {
            first.push(x);
        }
    }
    first.extend(last);
    first
}
