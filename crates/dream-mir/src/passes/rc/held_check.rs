//! Coverage of snapshot uses by an unchanged live strong owner.

use super::super::liveness::{add_terminator_reads, transfer_stmt};
use super::*;
use crate::analysis::effects::{Effect, ModRef, stmt_effects};
use crate::analysis::ownership::Location;
use dream_types::TyKind;

pub(super) struct Check<'a> {
    pub(super) observes: bool,
    pub(super) token_live: &'a [IndexSet<u32>],
    pub(super) facts: &'a OwnershipFacts,
    pub(super) f: &'a MirFunction,
    pub(super) interner: &'a TypeInterner,
    pub(super) layouts: &'a LayoutTable,
    pub(super) modref: &'a ModRefTable,
    pub(super) x: u32,
    pub(super) snap: Snapshot,
    pub(super) class: &'a BTreeSet<u32>,
    /// Pending snapshots, whose RC ops do not count as reads.
    pub(super) ignored: &'a IndexSet<u32>,
}

impl Check<'_> {
    pub(super) fn holds(&self, live_out: &[IndexSet<u32>]) -> bool {
        // Cleanup can be observable after the last value read. Such snapshots need
        // their owner and slot protected through the original token's actual release.
        let preserves_cleanup = self.modref.may_run_del(
            self.f.local_ty(crate::Local(self.x)),
            self.interner,
            self.layouts,
        );
        let observations = if self.observes && !preserves_cleanup {
            self.observation_reads()
        } else {
            BTreeMap::new()
        };
        let extended_live = if observations.is_empty() {
            None
        } else {
            let mut observed = without_rc_on(self.f, self.ignored);
            for (bi, block) in observed.blocks.iter_mut().enumerate() {
                block.stmts.clear();
                for (si, stmt) in self.f.blocks[bi].stmts.iter().enumerate() {
                    if let Some(reads) = observations.get(&(bi, si)) {
                        block.stmts.extend(reads.iter().map(|&local| {
                            Statement::Retain(Operand::Copy(Place::Local(crate::Local(local))))
                        }));
                    }
                    if rc_target(stmt).is_none_or(|local| !self.ignored.contains(&local)) {
                        block.stmts.push(stmt.clone());
                    }
                }
            }
            Some(liveness::live_out(&observed))
        };
        let live_out = if preserves_cleanup {
            self.token_live
        } else {
            extended_live.as_deref().unwrap_or(live_out)
        };
        let base = self.snap.base;
        if self
            .interner
            .is_shared_type(self.f.local_ty(crate::Local(base)))
        {
            return false;
        }
        let pinned =
            self.f.params.iter().any(|p| p.0 == base) && !self.f.locals[base as usize].is_take;
        let pins = self.owner_pins();
        let owner_pinned = pins.iter().any(|&owner| {
            self.f.params.iter().any(|p| p.0 == owner) && !self.f.locals[owner as usize].is_take
        });
        let base_ok = |live: &IndexSet<u32>, bi, si| {
            !self.family_live(live, bi, si)
                || pinned
                || live.contains(&base)
                || owner_pinned
                || pins.iter().any(|owner| live.contains(owner))
        };
        for (bi, block) in self.f.blocks.iter().enumerate() {
            let mut live = live_out[bi].clone();
            if !base_ok(&live, bi, block.stmts.len()) {
                return false;
            }
            add_terminator_reads(&block.terminator, &mut live);
            if !base_ok(&live, bi, block.stmts.len()) {
                return false;
            }
            for (si, stmt) in block.stmts.iter().enumerate().rev() {
                let ignored_rc = !preserves_cleanup
                    && rc_target(stmt).is_some_and(|l| self.ignored.contains(&l));
                if let Some(reads) = observations.get(&(bi, si)) {
                    live.extend(reads);
                }
                let in_flight = self.family_live(&live, bi, si + 1)
                    || (!ignored_rc
                        && self.class.iter().any(|&m| {
                            self.facts.may_be_nonnull(bi, si, crate::Local(m))
                                && super::super::stmt_reads_local(stmt, m)
                        }));
                if in_flight && !self.stmt_ok(bi, si, stmt) {
                    return false;
                }
                if !ignored_rc {
                    transfer_stmt(stmt, &mut live);
                }
                if !base_ok(&live, bi, si) {
                    return false;
                }
            }
        }
        true
    }

    fn family_live(&self, live: &IndexSet<u32>, bi: usize, si: usize) -> bool {
        self.class
            .iter()
            .any(|&m| live.contains(&m) && self.facts.may_be_nonnull(bi, si, crate::Local(m)))
    }

    fn observation_reads(&self) -> BTreeMap<(usize, usize), IndexSet<u32>> {
        let mut reads = BTreeMap::new();
        for (bi, block) in self.f.blocks.iter().enumerate() {
            let mut live = self.token_live[bi].clone();
            add_terminator_reads(&block.terminator, &mut live);
            for (si, stmt) in block.stmts.iter().enumerate().rev() {
                if crate::analysis::ownership::statement_observes_ownership(
                    self.f,
                    self.interner,
                    self.layouts,
                    self.modref,
                    stmt,
                ) {
                    let observed: IndexSet<_> = self
                        .class
                        .iter()
                        .copied()
                        .filter(|&m| {
                            self.facts.may_be_nonnull(bi, si, crate::Local(m))
                                && (live.contains(&m) || super::super::stmt_reads_local(stmt, m))
                        })
                        .collect();
                    if !observed.is_empty() {
                        reads.insert((bi, si), observed);
                    }
                }
                transfer_stmt(stmt, &mut live);
            }
        }
        reads
    }

    /// `stmt` runs while `x` is in use.
    fn stmt_ok(&self, bi: usize, si: usize, stmt: &Statement) -> bool {
        let base = self.snap.base;
        let pins = self.owner_pins();
        let mut base_given = false;
        for_each_given_away(stmt, |l| base_given |= l == base || pins.contains(&l));
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
        if base_given
            || base_dropped
            || matches!(stmt, Statement::RegionLeave | Statement::DebugLine(_))
        {
            return false;
        }
        let may_del = |l: u32| {
            self.modref
                .may_run_del(self.f.locals[l as usize].ty, self.interner, self.layouts)
        };
        let drops = match stmt {
            Statement::Release(op) => local_of(op).is_none_or(|l| l != self.x && may_del(l)),
            Statement::ValueDrop(l) => may_del(l.0),
            Statement::DeferLeave(_) => true,
            _ => self
                .modref
                .statement_may_destroy(self.f, stmt, self.interner, self.layouts),
        };
        if drops && self.summary_hits(self.modref.del()) {
            return false;
        }
        let mut ok = true;
        stmt_effects(stmt, self.f, self.interner, |e| {
            ok &= match e {
                Effect::None | Effect::GlobalStore(_) => true,
                Effect::Top | Effect::ObserveOwnership => false,
                Effect::Store(b, field) => {
                    !(self.snap.slot == Slot::Field(self.f.local_ty(b), field)
                        && self.facts.may_alias(bi, si, b, crate::Local(base)))
                        && !matches!(self.snap.slot, Slot::ElemField(arr, f)
                            if self.interner.kind(arr) == &TyKind::Array(self.f.local_ty(b)) && f == field)
                        && !self.pin_locations().iter().any(|&(owner, f)| {
                            f == field && self.facts.may_alias(bi, si, b, crate::Local(owner))
                        })
                }
                Effect::SlotStore(ty) => {
                    !self.hits_elem(ty)
                        || match stmt {
                            Statement::Assign(
                                Place::Index {
                                    base: written,
                                    index,
                                    ..
                                },
                                _,
                            ) => {
                                !self.facts.may_alias(bi, si, *written, crate::Local(base))
                                    || self
                                        .snap
                                        .index
                                        .zip(constant_index(index))
                                        .is_some_and(|(a, b)| a != b)
                            }
                            _ => false,
                        }
                }
                Effect::Call(c) => {
                    let args = match stmt {
                        Statement::Call { args, .. }
                        | Statement::Assign(_, Rvalue::Call { args, .. }) => args.as_slice(),
                        _ => &[],
                    };
                    !self.call_hits(&self.modref.call_at(c, args), bi, si)
                }
                Effect::Iface(id, slot) => !self.call_hits(&self.modref.iface(id, slot), bi, si),
                Effect::Ctor(def) => !self.call_hits(&self.modref.ctor(def), bi, si),
            };
        });
        ok
    }

    fn call_hits(&self, summary: &ModRef, bi: usize, si: usize) -> bool {
        let base = crate::Local(self.snap.base);
        let location = match self.snap.slot {
            Slot::Field(_, field) => Some(Location::Field { base, field }),
            Slot::Elem(_) => Some(Location::Element {
                base,
                index: self.snap.index,
            }),
            Slot::ElemField(..) => {
                return self.summary_hits(summary)
                    || (summary.may_destroy() && self.summary_hits(self.modref.del()));
            }
            Slot::Payload | Slot::Param => None,
        };
        let hits = location
            .into_iter()
            .chain(
                self.pin_locations()
                    .into_iter()
                    .map(|(base, field)| Location::Field {
                        base: crate::Local(base),
                        field,
                    }),
            )
            .any(|location| {
                let mut protected = summary.clone();
                if let ModRef::Known(k) = &mut protected {
                    k.observes_heap = false;
                }
                let stmt = &self.f.blocks[bi].stmts[si];
                match stmt {
                    Statement::Call { args, .. }
                    | Statement::Assign(_, Rvalue::Call { args, .. }) => self.facts.call_hits(
                        self.f,
                        self.interner,
                        &protected,
                        args,
                        (bi, si),
                        location,
                    ),
                    _ => self.summary_hits(&protected),
                }
            });
        let observes = match summary {
            ModRef::Top => true,
            ModRef::Known(k) => k.observes_rc,
        };
        // Unrelated destructors cannot invalidate a snapshot across a call with no cleanup.
        hits || observes || (summary.may_destroy() && self.summary_hits(self.modref.del()))
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
        for _ in 0..self.f.locals.len() {
            let Some((src, _)) = self.owner_source(cur) else {
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

    fn owner_source(&self, local: u32) -> Option<(u32, Option<u32>)> {
        if let Some(src) = self.copy_src(local) {
            return Some((src, None));
        }
        if !self.f.locals[local as usize].is_cursor {
            return None;
        }
        let mut source = None;
        for stmt in self.f.blocks.iter().flat_map(|b| &b.stmts) {
            if let Statement::Assign(Place::Local(d), rv) = stmt
                && d.0 == local
                && !matches!(rv, Rvalue::Use(Operand::Const(Const::Null)))
            {
                let Rvalue::Use(Operand::Copy(Place::Field { base, field })) = rv else {
                    return None;
                };
                if !self.interner.is_reference(self.f.local_ty(*base)) {
                    return None;
                }
                let layout = self
                    .layouts
                    .get(self.f.local_ty(*base))?
                    .fields
                    .get(*field)?;
                if layout.is_weak || layout.is_unowned {
                    return None;
                }
                let next = (base.0, Some(*field as u32));
                if source.is_some_and(|prior| prior != next) {
                    return None;
                }
                source = Some(next);
            }
        }
        source
    }

    fn pin_fields(&self) -> Vec<(TypeId, u32)> {
        self.pin_locations()
            .into_iter()
            .map(|(owner, field)| (self.f.locals[owner as usize].ty, field))
            .collect()
    }

    fn pin_locations(&self) -> Vec<(u32, u32)> {
        let mut fields = Vec::new();
        if let Some((owner, field)) = self.field_pin() {
            fields.push((owner, field));
        }
        for owner in self.owner_pins() {
            if let Some((base, Some(field))) = self.owner_source(owner) {
                fields.push((base, field));
            }
        }
        fields
    }

    /// Array pointer loaded only from one field of one object. The element stays alive while
    /// that object does, even when the pointer local itself is dead between reloads.
    fn field_pin(&self) -> Option<(u32, u32)> {
        if !matches!(self.snap.slot, Slot::Elem(_) | Slot::ElemField(..)) {
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
            Slot::Elem(arr) | Slot::ElemField(arr, _) => match self.interner.kind(arr) {
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
        if self
            .pin_fields()
            .iter()
            .any(|field| k.fields.contains(field))
        {
            return true;
        }
        match self.snap.slot {
            Slot::Field(ty, field) => k.fields.contains(&(ty, field)),
            Slot::Elem(_) => k.any_slot || k.slots.iter().any(|&t| self.hits_elem(t)),
            Slot::ElemField(arr, field) => {
                k.any_slot
                    || k.slots.iter().any(|&t| self.hits_elem(t))
                    || matches!(self.interner.kind(arr), TyKind::Array(elem)
                        if k.fields.contains(&(*elem, field)))
            }
            Slot::Payload | Slot::Param => false,
        }
    }
}
