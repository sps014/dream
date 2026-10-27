//! Program-point provenance for ownership proofs. Heap projections remain unknown: two
//! disjoint containers can own the same child, so parent identity is never child identity.

use super::effects::{Effect, ModRef, ModRefTable, stmt_effects};
use crate::{Const, Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;

#[derive(Clone, Copy)]
pub(crate) enum Location {
    Field { base: Local, field: u32 },
    Element { base: Local, index: Option<i64> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Origin {
    Parameter(usize),
    Allocation(usize, usize),
    Unknown,
}

type Origins = Rc<BTreeSet<Origin>>;
type State = Vec<Origins>;

pub(crate) struct OwnershipFacts {
    slots: Vec<Option<usize>>,
    entries: Vec<State>,
    definitions: Vec<BTreeMap<Local, Vec<(usize, Origins)>>>,
    opaque: bool,
}

pub(crate) fn observes_ownership(
    f: &MirFunction,
    i: &TypeInterner,
    layouts: &dream_hir::LayoutTable,
    table: &ModRefTable,
) -> bool {
    f.blocks
        .iter()
        .flat_map(|b| &b.stmts)
        .any(|stmt| statement_observes_ownership(f, i, layouts, table, stmt))
}

pub(crate) fn statement_observes_ownership(
    f: &MirFunction,
    i: &TypeInterner,
    layouts: &dream_hir::LayoutTable,
    table: &ModRefTable,
    stmt: &Statement,
) -> bool {
    let observes = |summary: &ModRef| match summary {
        ModRef::Top => true,
        ModRef::Known(k) => k.observes_ownership(),
    };
    let mut hit = matches!(stmt, Statement::DebugLine(_));
    stmt_effects(stmt, f, i, |e| {
        let summary = match e {
            Effect::Top | Effect::ObserveOwnership => {
                hit = true;
                return;
            }
            Effect::Call(c) => {
                let arguments = match stmt {
                    Statement::Call { args, .. }
                    | Statement::Assign(_, Rvalue::Call { args, .. }) => args.as_slice(),
                    _ => &[],
                };
                hit |= table.observes_call_at(c, arguments)
                    || (table.call_may_destroy(c.def, &c.args) && table.del_observes);
                return;
            }
            Effect::Iface(id, slot) => table.iface(id, slot),
            Effect::Ctor(def) => {
                hit |=
                    table.observes_ctor(def) || (table.ctor_may_destroy(def) && table.del_observes);
                return;
            }
            _ => return,
        };
        hit |= observes(&summary) || (summary.may_destroy() && table.del_observes);
    });
    let cleanup = table.statement_may_destroy(f, stmt, i, layouts) && table.del_observes;
    hit || cleanup
}

impl OwnershipFacts {
    pub(crate) fn unknown() -> Self {
        Self {
            slots: vec![],
            entries: vec![],
            definitions: vec![],
            opaque: true,
        }
    }
    pub(crate) fn invalidates(
        &self,
        f: &MirFunction,
        i: &TypeInterner,
        table: &ModRefTable,
        point: (usize, usize),
        location: Location,
    ) -> bool {
        let (bi, si) = point;
        let stmt = &f.blocks[bi].stmts[si];
        let mut hit = matches!(
            stmt,
            Statement::DebugLine(_)
                | Statement::RegionLeave
                | Statement::ForceFree(_)
                | Statement::DeferLeave(_)
        );
        stmt_effects(stmt, f, i, |effect| {
            hit |= match effect {
                Effect::None | Effect::GlobalStore(_) => false,
                Effect::Top | Effect::ObserveOwnership => true,
                Effect::Store(written, field) => match location {
                    Location::Field {
                        base,
                        field: protected,
                    } => {
                        field == protected
                            && f.local_ty(base) == f.local_ty(written)
                            && self.may_alias(bi, si, base, written)
                    }
                    Location::Element { .. } => false,
                },
                Effect::SlotStore(ty) => match location {
                    Location::Field { .. } => false,
                    Location::Element { base, index } => {
                        if !matches!(i.kind(f.local_ty(base)), dream_types::TyKind::Array(e) if *e == ty)
                        {
                            false
                        } else if let Statement::Assign(
                            Place::Index {
                                base: written,
                                index: at,
                                ..
                            },
                            _,
                        ) = stmt
                        {
                            self.may_alias(bi, si, base, *written)
                                && !index.zip(constant_index(at)).is_some_and(|(a, b)| a != b)
                        } else {
                            true
                        }
                    }
                },
                Effect::Call(c) => {
                    let args = match stmt {
                        Statement::Call { args, .. }
                        | Statement::Assign(_, Rvalue::Call { args, .. }) => args.as_slice(),
                        _ => &[],
                    };
                    self.call_hits(f, i, &table.call_at(c, args), args, point, location)
                }
                Effect::Iface(id, slot) => summary_hits(f, i, &table.iface(id, slot), location),
                Effect::Ctor(def) => summary_hits(f, i, &table.ctor(def), location),
            };
        });
        hit
    }

    pub(crate) fn call_hits(
        &self,
        f: &MirFunction,
        i: &TypeInterner,
        summary: &ModRef,
        args: &[Operand],
        point: (usize, usize),
        location: Location,
    ) -> bool {
        let (ModRef::Known(k), Location::Field { base, field }) = (summary, location) else {
            return summary_hits(f, i, summary, location);
        };
        if k.observes_ownership() {
            return true;
        }
        let key = (f.local_ty(base), field);
        if !k.fields.contains(&key) {
            return false;
        }
        let Some(Some(parameters)) = k.field_params.get(&key) else {
            return true;
        };
        parameters
            .iter()
            .any(|&parameter| match args.get(parameter) {
                Some(Operand::Copy(Place::Local(actual))) => {
                    self.may_alias(point.0, point.1, base, *actual)
                }
                _ => true,
            })
    }

    pub(crate) fn compute(f: &MirFunction, i: &TypeInterner) -> Self {
        let _span = tracing::debug_span!("ownership_origins", function = %f.name,
            locals = f.locals.len(), blocks = f.blocks.len())
        .entered();
        // `ref` aliases inline value storage, never a heap-reference local's pointer slot.
        // References loaded through that storage still receive unknown provenance.
        let mut count = 0;
        let slots: Vec<_> = f
            .locals
            .iter()
            .map(|d| {
                i.is_reference(d.ty).then(|| {
                    let slot = count;
                    count += 1;
                    slot
                })
            })
            .collect();
        if count == 0 {
            return Self::unknown();
        }
        let mut entries: Vec<Option<State>> = vec![None; f.blocks.len()];
        let mut entry = vec![Origins::default(); count];
        for (index, p) in f.params.iter().enumerate() {
            if let Some(slot) = slots[p.0 as usize] {
                Rc::make_mut(&mut entry[slot]).insert(Origin::Parameter(index));
            }
        }
        entries[f.entry.0 as usize] = Some(entry);
        let mut queue = VecDeque::from([f.entry.0 as usize]);
        let mut queued = vec![false; f.blocks.len()];
        queued[f.entry.0 as usize] = true;
        while let Some(bi) = queue.pop_front() {
            queued[bi] = false;
            let mut state = entries[bi].clone().expect("reachable worklist entry");
            for (si, stmt) in f.blocks[bi].stmts.iter().enumerate() {
                transfer(bi, si, stmt, &mut state, &slots);
            }
            if let Terminator::Await { dest: Some(d), .. } = f.blocks[bi].terminator
                && let Some(slot) = slots[d.0 as usize]
            {
                state[slot] = Rc::new(BTreeSet::from([Origin::Unknown]));
            }
            for next in f.blocks[bi].terminator.successors() {
                let next = next.0 as usize;
                let changed = match &mut entries[next] {
                    None => {
                        entries[next] = Some(state.clone());
                        true
                    }
                    Some(entry) => {
                        let mut changed = false;
                        for (dst, src) in entry.iter_mut().zip(&state) {
                            changed |= join_origins(dst, src);
                        }
                        changed
                    }
                };
                if changed && !queued[next] {
                    queued[next] = true;
                    queue.push_back(next);
                }
            }
        }
        let entries: Vec<State> = entries
            .into_iter()
            .map(|entry| {
                entry.unwrap_or_else(|| {
                    let unknown = Rc::new(BTreeSet::from([Origin::Unknown]));
                    vec![unknown; count]
                })
            })
            .collect();
        let definitions = f
            .blocks
            .iter()
            .enumerate()
            .map(|(bi, block)| {
                let mut state = entries[bi].clone();
                let mut definitions: BTreeMap<Local, Vec<(usize, Origins)>> = BTreeMap::new();
                for (si, stmt) in block.stmts.iter().enumerate() {
                    transfer(bi, si, stmt, &mut state, &slots);
                    if let Statement::Assign(Place::Local(d), _) = stmt
                        && let Some(slot) = slots[d.0 as usize]
                    {
                        definitions
                            .entry(*d)
                            .or_default()
                            .push((si, state[slot].clone()));
                    }
                }
                definitions
            })
            .collect();
        Self {
            slots,
            entries,
            definitions,
            opaque: false,
        }
    }

    fn origins(&self, bi: usize, si: usize, local: Local) -> Option<&Origins> {
        let slot = self.slots.get(local.0 as usize).copied().flatten()?;
        let events = self.definitions[bi].get(&local);
        if let Some(events) = events {
            let at = events.partition_point(|(index, _)| *index < si);
            if at != 0 {
                return Some(&events[at - 1].1);
            }
        }
        Some(&self.entries[bi][slot])
    }

    pub(crate) fn parameter_origins(
        &self,
        bi: usize,
        si: usize,
        local: Local,
    ) -> Option<BTreeSet<usize>> {
        if self.opaque {
            return None;
        }
        let origins = self.origins(bi, si, local)?;
        if origins.is_empty() {
            return None;
        }
        origins
            .iter()
            .map(|origin| match origin {
                Origin::Parameter(index) => Some(Some(*index)),
                Origin::Allocation(..) => Some(None),
                Origin::Unknown => None,
            })
            .collect::<Option<Vec<_>>>()
            .map(|roots| roots.into_iter().flatten().collect())
    }

    pub(crate) fn may_be_nonnull(&self, bi: usize, si: usize, local: Local) -> bool {
        self.opaque
            || self
                .origins(bi, si, local)
                .is_none_or(|origins| !origins.is_empty())
    }

    pub(crate) fn may_alias(&self, bi: usize, si: usize, left: Local, right: Local) -> bool {
        if self.opaque || left == right {
            return true;
        }
        let (Some(a), Some(b)) = (self.origins(bi, si, left), self.origins(bi, si, right)) else {
            return true;
        };
        // Empty provenance also covers synthesized/uninitialized locals in MIR fixtures.
        if a.is_empty() || b.is_empty() {
            return true;
        }
        a.iter().any(|a| {
            b.iter().any(|b| match (a, b) {
                (Origin::Unknown, _) | (_, Origin::Unknown) => true,
                (Origin::Parameter(_), Origin::Parameter(_)) => true,
                _ => a == b,
            })
        })
    }
}

pub(crate) fn constant_index(op: &Operand) -> Option<i64> {
    match op {
        Operand::Const(Const::Int(n)) => Some(*n),
        _ => None,
    }
}

fn summary_hits(f: &MirFunction, i: &TypeInterner, summary: &ModRef, location: Location) -> bool {
    let ModRef::Known(k) = summary else {
        return true;
    };
    k.observes_ownership()
        || match location {
            Location::Field { base, field } => summary.writes_field(f.local_ty(base), field),
            Location::Element { base, .. } => match i.kind(f.local_ty(base)) {
                dream_types::TyKind::Array(e) => summary.writes_element(*e),
                _ => true,
            },
        }
}

fn operand(op: &Operand, state: &State, slots: &[Option<usize>]) -> Origins {
    match op {
        Operand::Copy(Place::Local(l)) => slots[l.0 as usize]
            .map(|slot| state[slot].clone())
            .unwrap_or_else(|| Rc::new(BTreeSet::from([Origin::Unknown]))),
        Operand::Const(Const::Null) => Origins::default(),
        _ => Rc::new(BTreeSet::from([Origin::Unknown])),
    }
}

fn join_origins(dst: &mut Origins, src: &Origins) -> bool {
    // Unknown already aliases every object; preserving additional sites only inflates
    // loop-carried states without adding information to any ownership query.
    if Rc::ptr_eq(dst, src) || dst.contains(&Origin::Unknown) {
        false
    } else if src.contains(&Origin::Unknown) {
        *dst = src.clone();
        true
    } else if src.iter().any(|origin| !dst.contains(origin)) {
        Rc::make_mut(dst).extend(src.iter().copied());
        true
    } else {
        false
    }
}

fn transfer(bi: usize, si: usize, stmt: &Statement, state: &mut State, slots: &[Option<usize>]) {
    if let Statement::Assign(Place::Local(d), rv) = stmt
        && let Some(slot) = slots[d.0 as usize]
    {
        state[slot] = match rv {
            Rvalue::Use(op) | Rvalue::Cast(op, ..) => operand(op, state, slots),
            Rvalue::Move { src, .. } => operand(&Operand::Copy(Place::Local(*src)), state, slots),
            Rvalue::Select {
                then_val, else_val, ..
            } => {
                let mut origins = operand(then_val, state, slots);
                join_origins(&mut origins, &operand(else_val, state, slots));
                origins
            }
            Rvalue::New { .. } | Rvalue::ArrayNew { .. } | Rvalue::ArrayLit { .. } => {
                Rc::new(BTreeSet::from([Origin::Allocation(bi, si)]))
            }
            _ => Rc::new(BTreeSet::from([Origin::Unknown])),
        };
    }
}

#[cfg(test)]
mod domain_tests {
    use super::*;

    #[test]
    fn unknown_absorbs_sites_and_null_without_losing_alias_information() {
        let unknown = Rc::new(BTreeSet::from([Origin::Unknown]));
        let mut origins = Rc::new(BTreeSet::from([
            Origin::Parameter(0),
            Origin::Allocation(0, 0),
        ]));
        assert!(join_origins(&mut origins, &unknown));
        for site in 0..128 {
            let allocation = Rc::new(BTreeSet::from([Origin::Allocation(1, site)]));
            assert!(!join_origins(&mut origins, &allocation));
        }
        assert!(!join_origins(&mut origins, &Origins::default()));
        assert_eq!(*origins, *unknown);
    }
}
