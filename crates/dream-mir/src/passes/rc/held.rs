//! Post-inline "held by a live owner" RC removal.
//!
//! RC insertion runs before inlining and must treat every call as able to overwrite any heap
//! slot, so a snapshot loaded out of a container keeps its own count:
//!
//! ```text
//! x = base.f | base[i] | base as Some._0
//! Retain(x)
//! ... reads of x, calls ...
//! Release(x)
//! ```
//!
//! After inlining, most of those calls are gone or have a [`ModRefTable`] summary. The retain
//! is redundant when, wherever `x` is still to be read:
//!
//! - `base` is live (or a borrowed parameter) and is neither rebound, released nor given away,
//!   so its object and the strong slot `x` came from stay alive;
//! - nothing can overwrite that slot: no store to the same `(type, field)` or element type, no
//!   call / constructor / `del` whose summary may, nothing opaque, nothing reading refcounts;
//!
//! and `x` never hands its count on (moved into a container, passed as a sink, returned,
//! copied). Then every `Retain(x)` / `Release(x)` goes and `x` becomes a cursor, which makes the
//! backend retain `x` at any container store.

use super::liveness::{self, add_terminator_reads, transfer_stmt};
use super::modref::{stmt_effects, Effect, ModRef, ModRefTable};
use super::tokens::{assigns_local, sink_call_args};
use crate::{Const, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_hir::LayoutTable;
use dream_types::{TyKind, TypeId, TypeInterner};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// `--emit-mir` stage name.
pub(crate) const STAGE: &str = "rc-held-by-owner";

/// Runs over every synchronous function; returns whether anything changed.
pub(crate) fn run(mir: &mut Mir, interner: &TypeInterner) -> bool {
    let modref = ModRefTable::compute(mir, interner);
    let layouts = mir.layouts.clone();
    let mut changed = false;
    for f in &mut mir.functions {
        changed |= run_function(f, interner, &layouts, &modref);
    }
    changed
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Slot {
    /// `(base type, field)`.
    Field(TypeId, u32),
    /// An element of this array type.
    Elem(TypeId),
    /// An (immutable) union payload.
    Payload,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Snapshot {
    base: u32,
    slot: Slot,
}

pub(crate) fn run_function(
    f: &mut MirFunction,
    interner: &TypeInterner,
    layouts: &LayoutTable,
    modref: &ModRefTable,
) -> bool {
    if f.is_async
        || f
            .blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Await { .. }))
    {
        return false;
    }
    let mut pending = candidates(f, interner, layouts);
    let mut changed = false;
    // A snapshot of a snapshot is decided after its base, whose own RC ops may go first.
    while !pending.is_empty() {
        let ready: Vec<u32> = pending
            .iter()
            .filter(|(_, c)| !pending.contains_key(&c.snap.base))
            .map(|(&x, _)| x)
            .collect();
        if ready.is_empty() {
            break;
        }
        let mut ignored: HashSet<u32> = pending.keys().copied().collect();
        for c in pending.values() {
            ignored.extend(c.class.iter().copied());
        }
        let live_out = liveness::live_out(&without_rc_on(f, &ignored));
        let accepted: Vec<u32> = ready
            .iter()
            .copied()
            .filter(|&x| {
                Check {
                    f,
                    interner,
                    layouts,
                    modref,
                    x,
                    snap: pending[&x].snap,
                    class: &pending[&x].class,
                    ignored: &ignored,
                }
                .holds(&live_out)
            })
            .collect();
        let classes: Vec<BTreeSet<u32>> = accepted.iter().map(|&x| pending[&x].class.clone()).collect();
        for x in &ready {
            pending.remove(x);
        }
        for class in &classes {
            for b in &mut f.blocks {
                b.stmts
                    .retain(|s| rc_target(s).is_none_or(|l| !class.contains(&l)));
            }
            for &m in class {
                f.locals[m as usize].is_cursor = true;
            }
            changed = true;
        }
    }
    changed
}

/// A retained snapshot and its unretained local aliases, whose validity rides on its count.
#[derive(Clone, Debug)]
struct Candidate {
    snap: Snapshot,
    class: BTreeSet<u32>,
}

/// Owned RC locals whose every definition is a load through the same base and slot followed by
/// its own `Retain`, with no other retain and no hand-off of the count by it or an alias.
fn candidates(
    f: &MirFunction,
    interner: &TypeInterner,
    layouts: &LayoutTable,
) -> BTreeMap<u32, Candidate> {
    let params: HashSet<u32> = f.params.iter().map(|p| p.0).collect();
    let eligible = |x: u32| {
        let d = &f.locals[x as usize];
        !params.contains(&x)
            && !d.is_cursor
            && interner.is_reference(d.ty)
            && !interner.is_shared_type(d.ty)
    };
    let mut snaps: HashMap<u32, Snapshot> = HashMap::new();
    let mut bad_snapshot: HashSet<u32> = HashSet::new();
    let mut given_away: HashSet<u32> = HashSet::new();
    let mut defs: HashMap<u32, usize> = HashMap::new();
    let mut rc_ops: HashMap<u32, (usize, usize)> = HashMap::new();
    // Local-to-local copies `dest = src` not followed by a retain of `dest`.
    let mut alias_defs: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut other_defs: HashSet<u32> = HashSet::new();
    for block in &f.blocks {
        for (si, stmt) in block.stmts.iter().enumerate() {
            match stmt {
                Statement::Assign(Place::Local(d), rv)
                    if !matches!(rv, Rvalue::Use(Operand::Const(Const::Null))) =>
                {
                    let retained = retained_after(&block.stmts[si + 1..], d.0);
                    match rv {
                        Rvalue::Use(op) | Rvalue::Cast(op, _, _)
                            if local_of(op).is_some_and(|s| s != d.0) && !retained =>
                        {
                            alias_defs.entry(d.0).or_default().push(local_of(op).expect("some"));
                        }
                        _ => {
                            other_defs.insert(d.0);
                        }
                    }
                    if eligible(d.0) {
                        *defs.entry(d.0).or_default() += 1;
                        let ok = retained
                            && load_of(f, rv, layouts).is_some_and(|s| {
                                s.base != d.0 && *snaps.entry(d.0).or_insert(s) == s
                            });
                        if !ok {
                            bad_snapshot.insert(d.0);
                        }
                    }
                }
                Statement::Retain(Operand::Copy(Place::Local(l))) => {
                    rc_ops.entry(l.0).or_default().0 += 1;
                }
                Statement::Release(Operand::Copy(Place::Local(l))) => {
                    rc_ops.entry(l.0).or_default().1 += 1;
                }
                _ => {}
            }
            for_each_given_away(stmt, |l| {
                given_away.insert(l);
            });
        }
        if let Some(l) = terminator_hands_on(&block.terminator) {
            given_away.insert(l);
        }
    }
    let mut out = BTreeMap::new();
    for (x, snap) in snaps {
        if bad_snapshot.contains(&x) || rc_ops.get(&x).map(|r| r.0) != defs.get(&x).copied() {
            continue;
        }
        let mut class: BTreeSet<u32> = BTreeSet::from([x]);
        let mut ok = true;
        let mut grew = true;
        while grew && ok {
            grew = false;
            for (&y, srcs) in &alias_defs {
                if class.contains(&y) || !srcs.iter().any(|s| class.contains(s)) {
                    continue;
                }
                // An alias that retained for itself holds a second count. A release with no
                // retain is the snapshot's count, moved into the alias (`op = elem`).
                let own_retain = rc_ops.get(&y).is_some_and(|r| r.0 != 0);
                if params.contains(&y)
                    || other_defs.contains(&y)
                    || own_retain
                    || !srcs.iter().all(|s| class.contains(s))
                {
                    ok = false;
                    break;
                }
                class.insert(y);
                grew = true;
            }
        }
        if ok && !class.iter().any(|m| given_away.contains(m)) {
            out.insert(x, Candidate { snap, class });
        }
    }
    out
}

fn load_of(f: &MirFunction, rv: &Rvalue, layouts: &LayoutTable) -> Option<Snapshot> {
    match rv {
        Rvalue::Use(Operand::Copy(Place::Field { base, field })) => {
            let ty = f.local_ty(*base);
            let fl = layouts.get(ty)?.fields.get(*field)?;
            if fl.is_weak || fl.is_unowned {
                return None;
            }
            Some(Snapshot {
                base: base.0,
                slot: Slot::Field(ty, *field as u32),
            })
        }
        Rvalue::Use(Operand::Copy(Place::Index { base, .. })) => Some(Snapshot {
            base: base.0,
            slot: Slot::Elem(f.local_ty(*base)),
        }),
        Rvalue::UnionField {
            base: Operand::Copy(Place::Local(b)),
            ..
        } => Some(Snapshot {
            base: b.0,
            slot: Slot::Payload,
        }),
        _ => None,
    }
}

/// `Retain(x)` follows the definition before anything else touches `x`.
fn retained_after(rest: &[Statement], x: u32) -> bool {
    for s in rest {
        if matches!(s, Statement::Retain(_)) && rc_target(s) == Some(x) {
            return true;
        }
        if super::stmt_reads_local(s, x) || assigns_local(s, x) {
            return false;
        }
    }
    false
}

fn rc_target(s: &Statement) -> Option<u32> {
    match s {
        Statement::Retain(Operand::Copy(Place::Local(l)))
        | Statement::Release(Operand::Copy(Place::Local(l))) => Some(l.0),
        _ => None,
    }
}

fn local_of(op: &Operand) -> Option<u32> {
    match op {
        Operand::Copy(Place::Local(l)) => Some(l.0),
        _ => None,
    }
}

/// Locals whose count `stmt` may take over: sink arguments, container moves, aggregate
/// payloads, unconditional frees.
fn for_each_given_away(stmt: &Statement, mut out: impl FnMut(u32)) {
    if let Some((takes, args)) = sink_call_args(stmt) {
        for (i, a) in args.iter().enumerate() {
            if takes.get(i).copied().unwrap_or(false) {
                local_of(a).into_iter().for_each(&mut out);
            }
        }
    }
    match stmt {
        Statement::Assign(_, rv) => match rv {
            Rvalue::Move { src, .. } => out(src.0),
            Rvalue::UnionNew { args, .. }
            | Rvalue::ArrayLit { elems: args, .. }
            | Rvalue::Tuple { elems: args, .. }
            | Rvalue::New {
                ctor: None, args, ..
            } => args.iter().filter_map(local_of).for_each(out),
            _ => {}
        },
        Statement::ReleaseUnique(op) | Statement::ForceFree(op) => {
            local_of(op).into_iter().for_each(out)
        }
        _ => {}
    }
}

fn terminator_hands_on(t: &Terminator) -> Option<u32> {
    match t {
        Terminator::Return(Some(op)) | Terminator::AsyncComplete(Some(op)) => local_of(op),
        Terminator::Await { future, .. } => local_of(future),
        _ => None,
    }
    .or_else(|| match t {
        Terminator::TailCall { args, .. } => args.iter().find_map(local_of),
        _ => None,
    })
}

/// `f` without RC ops on `locals`, so their liveness is that of their real reads.
fn without_rc_on(f: &MirFunction, locals: &HashSet<u32>) -> MirFunction {
    let mut g = MirFunction {
        def: f.def,
        instance: f.instance.clone(),
        name: String::new(),
        params: f.params.clone(),
        ret: f.ret,
        locals: f.locals.clone(),
        blocks: f.blocks.clone(),
        entry: f.entry,
        is_async: f.is_async,
        hir_fn: None,
        file: None,
        prefer_inline: false,
    };
    for b in &mut g.blocks {
        b.stmts
            .retain(|s| !rc_target(s).is_some_and(|l| locals.contains(&l)));
    }
    g
}

struct Check<'a> {
    f: &'a MirFunction,
    interner: &'a TypeInterner,
    layouts: &'a LayoutTable,
    modref: &'a ModRefTable,
    x: u32,
    snap: Snapshot,
    class: &'a BTreeSet<u32>,
    /// Pending snapshots, whose RC ops do not count as reads.
    ignored: &'a HashSet<u32>,
}

impl Check<'_> {
    fn holds(&self, live_out: &[HashSet<u32>]) -> bool {
        let base = self.snap.base;
        let pinned =
            self.f.params.iter().any(|p| p.0 == base) && !self.f.locals[base as usize].is_take;
        let pins = self.owner_pins();
        let owner_pinned = pins.iter().any(|&owner| {
            self.f.params.iter().any(|p| p.0 == owner) && !self.f.locals[owner as usize].is_take
        });
        let base_ok = |live: &HashSet<u32>| {
            !self.class.iter().any(|m| live.contains(m))
                || pinned
                || live.contains(&base)
                || owner_pinned
                || pins.iter().any(|owner| live.contains(owner))
        };
        for (bi, block) in self.f.blocks.iter().enumerate() {
            let mut live = live_out[bi].clone();
            if !base_ok(&live) {
                return false;
            }
            add_terminator_reads(&block.terminator, &mut live);
            if !base_ok(&live) {
                return false;
            }
            for stmt in block.stmts.iter().rev() {
                let ignored_rc = rc_target(stmt).is_some_and(|l| self.ignored.contains(&l));
                let in_flight = self.class.iter().any(|&m| {
                    live.contains(&m) || (!ignored_rc && super::stmt_reads_local(stmt, m))
                });
                if in_flight && !self.stmt_ok(stmt) {
                    return false;
                }
                if !ignored_rc {
                    transfer_stmt(stmt, &mut live);
                }
                if !base_ok(&live) {
                    return false;
                }
            }
        }
        true
    }

    /// `stmt` runs while `x` is in use.
    fn stmt_ok(&self, stmt: &Statement) -> bool {
        let base = self.snap.base;
        let pins = self.owner_pins();
        let mut base_given = false;
        for_each_given_away(stmt, |l| {
            base_given |= l == base || pins.contains(&l)
        });
        let base_dropped = match stmt {
            // Reloading the array pointer from the same field does not free the buffer.
            Statement::Assign(Place::Local(d), rv) if d.0 == base => {
                !self.field_pin().is_some_and(|(owner, field)| {
                    matches!(
                        rv,
                        Rvalue::Use(Operand::Copy(Place::Field { base: o, field: f }))
                            if o.0 == owner && *f as u32 == field
                    )
                })
            }
            Statement::Assign(Place::Local(d), _) => pins.contains(&d.0),
            Statement::Release(op) => local_of(op).is_some_and(|l| l == base || pins.contains(&l)),
            Statement::ValueDrop(l) | Statement::ValueKill(l) => l.0 == base || pins.contains(&l.0),
            _ => false,
        };
        if base_given || base_dropped || matches!(stmt, Statement::RegionLeave) {
            return false;
        }
        let may_del = |l: u32| {
            self.modref
                .may_run_del(self.f.locals[l as usize].ty, self.interner, self.layouts)
        };
        let drops = match stmt {
            Statement::Release(op) | Statement::ReleaseUnique(op) => {
                local_of(op).is_none_or(|l| l != self.x && may_del(l))
            }
            Statement::ValueDrop(l) => may_del(l.0),
            Statement::DeferLeave(_) => true,
            _ => false,
        };
        if drops && self.summary_hits(self.modref.del()) {
            return false;
        }
        let mut ok = true;
        stmt_effects(stmt, self.f, self.interner, |e| {
            ok &= match e {
                Effect::None | Effect::GlobalStore(_) => true,
                Effect::Top => false,
                Effect::Store(b, field) => {
                    self.snap.slot != Slot::Field(self.f.local_ty(b), field)
                        && !self
                            .field_pin()
                            .is_some_and(|(owner, f)| b.0 == owner && field == f)
                }
                Effect::SlotStore(ty) => !self.hits_elem(ty),
                // A callee may also release anything, running any `del`.
                Effect::Call(c) => {
                    !self.summary_hits(&self.modref.call(c)) && !self.summary_hits(self.modref.del())
                }
                Effect::Iface(id, slot) => {
                    let summary = self.modref.iface(id, slot);
                    if summary.is_quiet() {
                        true
                    } else {
                        !self.summary_hits(&summary) && !self.summary_hits(self.modref.del())
                    }
                }
                Effect::Ctor(def) => {
                    !self.summary_hits(&self.modref.ctor(def))
                        && !self.summary_hits(self.modref.del())
                }
            };
        });
        ok
    }

    /// The field-load owner and every local it is copied from. Any one of them being live keeps
    /// the object alive; the immediate owner is often dead at a block edge that only forwards
    /// the loaded element.
    fn owner_pins(&self) -> Vec<u32> {
        let Some((owner, _)) = self.field_pin() else {
            return Vec::new();
        };
        let mut out = vec![owner];
        let mut cur = owner;
        for _ in 0..8 {
            let Some(src) = self.copy_src(cur) else {
                break;
            };
            if out.contains(&src) {
                break;
            }
            out.push(src);
            cur = src;
        }
        out
    }

    fn copy_src(&self, local: u32) -> Option<u32> {
        let mut src = None;
        let mut saw = false;
        for block in &self.f.blocks {
            for stmt in &block.stmts {
                let Statement::Assign(Place::Local(d), rv) = stmt else {
                    continue;
                };
                if d.0 != local || matches!(rv, Rvalue::Use(Operand::Const(Const::Null))) {
                    continue;
                }
                let Rvalue::Use(Operand::Copy(Place::Local(s))) = rv else {
                    return None;
                };
                if src.is_some_and(|p| p != s.0) {
                    return None;
                }
                src = Some(s.0);
                saw = true;
            }
        }
        if saw { src } else { None }
    }

    /// Array pointer loaded only from one field of one object. The element stays alive while
    /// that object does, even when the pointer local itself is dead between reloads.
    fn field_pin(&self) -> Option<(u32, u32)> {
        if !matches!(self.snap.slot, Slot::Elem(_)) {
            return None;
        }
        let base = self.snap.base;
        let mut pin: Option<(u32, u32)> = None;
        let mut saw = false;
        for block in &self.f.blocks {
            for stmt in &block.stmts {
                let Statement::Assign(Place::Local(d), rv) = stmt else {
                    continue;
                };
                if d.0 != base {
                    continue;
                }
                if matches!(rv, Rvalue::Use(Operand::Const(Const::Null))) {
                    continue;
                }
                let Rvalue::Use(Operand::Copy(Place::Field { base: owner, field })) = rv else {
                    return None;
                };
                let pair = (owner.0, *field as u32);
                if pin.is_some_and(|p| p != pair) {
                    return None;
                }
                pin = Some(pair);
                saw = true;
            }
        }
        if saw { pin } else { None }
    }

    fn hits_elem(&self, elem: TypeId) -> bool {
        match self.snap.slot {
            Slot::Elem(arr) => match self.interner.kind(arr) {
                TyKind::Array(e) => *e == elem,
                _ => true,
            },
            _ => false,
        }
    }

    fn summary_hits(&self, m: &ModRef) -> bool {
        let ModRef::Known(k) = m else {
            return true;
        };
        if k.observes_rc {
            return true;
        }
        match self.snap.slot {
            Slot::Field(ty, field) => k.fields.contains(&(ty, field)),
            Slot::Elem(_) => k.any_slot || k.slots.iter().any(|&t| self.hits_elem(t)),
            Slot::Payload => false,
        }
    }
}
