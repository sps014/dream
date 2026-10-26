//! What a call may do to its arguments, phrased over parameters so each caller can re-check it
//! against what it knows about the actual arguments.

use super::contents::Contents;
use super::state::State;
use super::value::{Atom, Atoms, PathId, Slot, insert, union};
use dream_types::TypeId;
use indexmap::IndexMap;
use smallvec::SmallVec;

/// The strong store a finding is about: `value` was stored into `slot` of a `container` object.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) struct Origin {
    pub(super) container: TypeId,
    pub(super) slot: Slot,
    pub(super) value: TypeId,
}

/// A store into storage the caller can observe: a parameter's graph, an old object, or an escaped
/// callee allocation. `safe` stores were proven unable to close a cycle for every caller.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(super) struct Write {
    pub(super) target: Atom,
    /// The slot written, when the store names one.
    pub(super) slot: Option<Slot>,
    pub(super) value: Atoms,
    /// False for an obligation that only asks callers to check `check` cannot reach `target`.
    pub(super) stores: bool,
    /// What callers must prove cannot reach `target` when the write is not `safe`: the
    /// parameters and old objects the value reached, or the whole value when `target` may
    /// denote one of the callee's own allocations. The callee already proved its allocations
    /// cannot reach `target` otherwise, so merging them into groups must not re-raise that.
    pub(super) check: Atoms,
    /// The value was handed to a holder outside every Dream object (a host or a global), which
    /// keeps it alive but can store nothing into any object.
    pub(super) retained: bool,
    pub(super) safe: bool,
    pub(super) origin: Option<Origin>,
    /// Source line of an unsafe store, for reporting it at an exported entry point.
    pub(super) line: u32,
}

/// Callee allocations a caller sees as one site. Allocations are grouped by whether they are
/// still private to the callee when it returns (its results and their insides) or were stored
/// into a parameter's graph or an old object, by whether they are returned, and by their exact
/// possible types, so objects of different types do not seem to contain each other. `Fresh(PRIVATE)` and `Fresh(SHARED)` hold
/// the untyped allocations and those past [`MAX_GROUPS`].
#[derive(Clone, Default, PartialEq, Debug)]
pub(super) struct FreshGroup {
    /// Union of everything stored into the group's objects.
    pub(super) contents: Contents,
    pub(super) allocates: bool,
    /// Escaped to code no summary replays.
    pub(super) lost: bool,
    /// See [`super::state::Site::stray`].
    pub(super) stray: bool,
    /// The types the group's objects may have, when `typed`; otherwise any type.
    pub(super) types: SmallVec<[TypeId; 2]>,
    pub(super) typed: bool,
}

pub(super) const PRIVATE: usize = 0;
pub(super) const SHARED: usize = 1;
/// Holder-sensitive groups per call; additional groups retain types without holder precision.
pub(super) const MAX_GROUPS: usize = 64;

#[derive(Clone, Default, PartialEq, Debug)]
pub(super) struct Summary {
    pub(super) writes: Vec<Write>,
    pub(super) ret: Atoms,
    pub(super) fresh: Vec<FreshGroup>,
    /// Slots written on objects the caller may reach, with the objects written.
    pub(super) slots: IndexMap<Slot, Atoms>,
    pub(super) all_slots: bool,
}

/// Larger summaries forget entry paths of detached objects ([`Summary::coarsen`]).
pub(super) const PRECISE_WRITES: usize = 256;
/// Summaries grow past this only for pathological bodies; they then fall back to [`havoc`].
pub(super) const MAX_WRITES: usize = 2048;

/// Allocation sites summarize as one group when they agree on escape, return, types and the
/// parameter handles holding them.
type GroupKey<'s> = (bool, bool, &'s [TypeId], &'s [Atom]);

impl Summary {
    /// Renames the walk's allocation sites to the groups callers see. `coarse` groups only by
    /// escape and return, which keeps the groups of a recursive fixpoint from trading objects
    /// every round.
    pub(super) fn finish(
        mut self,
        exit: &State,
        coarse: bool,
        managed: impl Fn(TypeId) -> bool,
    ) -> Summary {
        // Objects stored into different caller objects (say the new storage of two maps) stay
        // apart, so callers do not see one caller object holding the other's contents.
        let mut holders: Vec<Atoms> = vec![Atoms::new(); exit.sites.len()];
        let separate = self
            .writes
            .iter()
            .filter(|w| !coarse && w.stores && !matches!(w.target, Atom::Fresh(_)));
        for w in separate {
            for &atom in &w.value {
                if let Atom::Fresh(id) = atom
                    && let Some(set) = holders.get_mut(id as usize)
                {
                    insert(set, w.target);
                }
            }
        }
        let mut keys: IndexMap<GroupKey<'_>, usize> = IndexMap::new();
        let groups: Vec<usize> = exit
            .sites
            .iter()
            .enumerate()
            .map(|(id, site)| {
                let escaped = exit.escaped(id as u32);
                let base = if escaped { SHARED } else { PRIVATE };
                if !site.live || !site.typed {
                    return base;
                }
                let returned = self.ret.contains(&Atom::Fresh(id as u32));
                // Managed objects are barriers in reachability proofs. Merging them with
                // arbitrary overflow allocations erases that barrier and invents cycles.
                let managed = !site.types.is_empty() && site.types.iter().all(|&ty| managed(ty));
                let types: &[TypeId] = if coarse && !managed {
                    &[]
                } else {
                    site.types.as_slice()
                };
                // Once the holder budget is exhausted, retain type identity. Folding all
                // overflow objects into PRIVATE/SHARED invents edges between unrelated
                // allocations, including a server's request storage and its futures.
                let holder_set = if keys.len() + 2 < MAX_GROUPS {
                    holders[id].as_slice()
                } else {
                    &[]
                };
                let key = (escaped, returned, types, holder_set);
                let next = keys.len() + 2;
                match keys.get(&key) {
                    Some(&at) => at,
                    None => {
                        keys.insert(key, next);
                        next
                    }
                }
            })
            .collect();
        self.fresh = vec![FreshGroup::default(); keys.len() + 2];
        let group = |atom: Atom| match atom {
            Atom::Fresh(id) => Atom::Fresh(groups[id as usize] as u32),
            other => other,
        };
        let rename = |atoms: &Atoms| {
            let mut out = Atoms::new();
            for &atom in atoms {
                insert(&mut out, group(atom));
            }
            out
        };
        for w in std::mem::take(&mut self.writes) {
            self.record(Write {
                target: group(w.target),
                value: rename(&w.value),
                check: rename(&w.check),
                ..w
            });
        }
        self.ret = rename(&self.ret);
        for targets in self.slots.values_mut() {
            *targets = rename(targets);
        }
        let mut seen = vec![false; self.fresh.len()];
        for (site, &at) in exit.sites.iter().zip(&groups) {
            let fresh = &mut self.fresh[at];
            if site.live {
                if seen[at] {
                    fresh.typed &= site.typed;
                } else {
                    fresh.typed = site.typed;
                    seen[at] = true;
                }
                for &ty in &site.types {
                    if !fresh.types.contains(&ty) {
                        fresh.types.push(ty);
                    }
                }
                fresh.types.sort_unstable();
            }
            for (slot, atoms) in site.contents.buckets() {
                fresh.contents.add(slot, &rename(atoms));
            }
            fresh.allocates |= site.live;
            fresh.lost |= site.lost;
            fresh.stray |= site.stray;
        }
        self
    }

    /// Forgets which entry objects detached or relinked ones were. Recursion can keep growing
    /// those paths, so recursive groups fall back to this to converge.
    pub(super) fn coarsen(&mut self) {
        fn coarse(atom: Atom) -> Atom {
            match atom {
                Atom::Param(i, p) if p.is_cut() => Atom::Param(i, PathId::DETACHED),
                Atom::Param(i, p) if p.is_linked() => Atom::Param(i, PathId::BENEATH),
                other => other,
            }
        }
        fn all(atoms: &mut Atoms) {
            if atoms.iter().any(|&a| coarse(a) != a) {
                for atom in std::mem::take(atoms) {
                    insert(atoms, coarse(atom));
                }
            }
        }
        all(&mut self.ret);
        for targets in self.slots.values_mut() {
            all(targets);
        }
        for group in &mut self.fresh {
            for set in group.contents.sets_mut() {
                all(set);
            }
        }
        for mut w in std::mem::take(&mut self.writes) {
            w.target = coarse(w.target);
            all(&mut w.value);
            all(&mut w.check);
            self.record(w);
        }
    }

    /// Adds `write`, folding its value into an existing write that differs only in value, so
    /// stores the walk revisits with growing values stay one obligation.
    pub(super) fn record(&mut self, write: Write) {
        let existing = self.writes.iter_mut().find(|w| {
            w.target == write.target
                && w.slot == write.slot
                && w.stores == write.stores
                && w.retained == write.retained
                && w.safe == write.safe
                && w.origin == write.origin
                && w.line == write.line
        });
        match existing {
            Some(w) => {
                union(&mut w.value, &write.value);
                union(&mut w.check, &write.check);
            }
            None => self.writes.push(write),
        }
    }

    pub(super) fn wrote(&mut self, slot: Slot, targets: &[Atom]) {
        if !self.all_slots {
            union(self.slots.entry(slot).or_default(), targets);
        }
    }

    pub(super) fn wrote_all(&mut self) {
        self.all_slots = true;
        self.slots.clear();
    }
}

/// The weakest sound summary: any argument may be linked to any other or to old objects.
pub(super) fn havoc(params: &[(u32, TypeId)], ret_bearing: bool) -> Summary {
    let mut summary = Summary {
        all_slots: true,
        fresh: vec![FreshGroup::default(); 2],
        ..Summary::default()
    };
    let mut everything = Atoms::new();
    insert(&mut everything, Atom::Old);
    insert(&mut everything, Atom::Fresh(SHARED as u32));
    for &(index, _) in params {
        insert(&mut everything, Atom::Param(index, PathId::ANY));
    }
    let shared = &mut summary.fresh[SHARED];
    shared.contents.add(None, &everything);
    shared.allocates = true;
    shared.lost = true;
    shared.stray = true;
    if ret_bearing {
        summary.ret = everything.clone();
    }
    for &(target, container) in params {
        for &(_, value_ty) in params {
            summary.record(Write {
                target: Atom::Param(target, PathId::ANY),
                slot: None,
                value: everything.clone(),
                check: everything.clone(),
                stores: true,
                retained: false,
                safe: false,
                origin: Some(Origin {
                    container,
                    slot: Slot::Elem(container),
                    value: value_ty,
                }),
                line: 0,
            });
        }
    }
    summary.record(Write {
        target: Atom::Old,
        slot: None,
        check: everything.clone(),
        value: everything,
        stores: true,
        retained: false,
        safe: true,
        origin: None,
        line: 0,
    });
    summary
}

/// A store this analysis could not prove safe and no caller can resolve.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(super) struct Finding {
    pub(super) origin: Origin,
    pub(super) file: Option<String>,
    pub(super) line: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflowing_holder_budget_keeps_allocation_types_distinct() {
        let mut state = State::default();
        for id in 0..100 {
            let site = state.site_mut(id);
            site.live = true;
            site.set_types(true, &[TypeId(id)]);
        }
        let summary = Summary {
            ret: super::super::value::one(Atom::Fresh(99)),
            ..Default::default()
        }
        .finish(&state, false, |_| false);
        assert!(
            summary
                .fresh
                .iter()
                .all(|group| !group.allocates || (group.typed && group.types.len() == 1))
        );
        let Atom::Fresh(last) = summary.ret[0] else {
            panic!("expected allocation");
        };
        assert_eq!(summary.fresh[last as usize].types.as_slice(), &[TypeId(99)]);
    }

    #[test]
    fn coarse_summaries_preserve_managed_type_barriers() {
        let mut state = State::default();
        for id in 0..3 {
            let site = state.site_mut(id);
            site.live = true;
            site.set_types(true, &[TypeId(id)]);
        }
        let summary = Summary::default().finish(&state, true, |ty| ty == TypeId(2));
        assert!(
            summary
                .fresh
                .iter()
                .any(|group| group.typed && group.types.as_slice() == [TypeId(2)])
        );
    }
}
