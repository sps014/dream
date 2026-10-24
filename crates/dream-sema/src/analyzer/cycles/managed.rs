//! `@allow_cycle` classes: a store whose every cycle passes through an object of such a class
//! closes only cycles the program manages itself.

use super::state::Site;
use super::value::{Atom, Slot};
use super::walker::Walker;
use dream_types::TyKind;
use smallvec::SmallVec;

impl Walker<'_, '_> {
    fn managed(&self, id: u32) -> bool {
        self.state.site(id).is_some_and(|site| {
            site.typed
                && !site.types.is_empty()
                && site.types.iter().all(|&ty| {
                    matches!(self.prog.graph.interner().kind(ty),
                        TyKind::Struct(def, _) if self.prog.managed.contains(def))
                })
        })
    }

    /// Whether every way `value` can reach the objects `target` denotes runs through an object
    /// of an `@allow_cycle` class, so any cycle the store closes contains one.
    pub(super) fn managed_store(&self, target: Atom, slot: Option<Slot>, value: &[Atom]) -> bool {
        if self.prog.managed.is_empty() {
            return false;
        }
        if let Atom::Fresh(id) = target {
            return self.managed_cycle(id, value);
        }
        // Outside objects and allocations the handle may denote are reachable only through
        // managed objects.
        let mut seen: SmallVec<[u32; 16]> = SmallVec::new();
        let mut pending: SmallVec<[Atom; 16]> = value.iter().copied().collect();
        while let Some(atom) = pending.pop() {
            let Atom::Fresh(id) = atom else {
                return false;
            };
            if seen.contains(&id) || self.managed(id) {
                continue;
            }
            seen.push(id);
            let Some(site) = self.state.site(id) else {
                return false;
            };
            if site.may_hold_store(&self.prog.graph, self.paths, slot, &[target]) {
                return false;
            }
            pending.extend(site.contents.atoms(false));
            pending.extend(site.untracked.iter().copied());
        }
        true
    }

    fn managed_cycle(&self, target: u32, value: &[Atom]) -> bool {
        let managed = |id: u32| self.managed(id);
        if managed(target) {
            return true;
        }
        let mut seen: SmallVec<[u32; 16]> = SmallVec::new();
        let mut pending: SmallVec<[Atom; 16]> = value.iter().copied().collect();
        let mut outside = false;
        while let Some(atom) = pending.pop() {
            let Atom::Fresh(id) = atom else {
                outside = true;
                continue;
            };
            if id == target {
                return false;
            }
            if seen.contains(&id) || managed(id) {
                continue;
            }
            seen.push(id);
            if let Some(site) = self.state.site(id) {
                pending.extend(site.contents.atoms(false));
            }
            pending.extend(self.state.untracked_contents(id).iter().copied());
        }
        !outside || !self.exposed(target, managed)
    }

    /// Whether an object no site tracks may reach `target` without passing through a site
    /// `shield` accepts: some site on a path of tracked holders back from it is `held`.
    fn exposed(&self, target: u32, shield: impl Fn(u32) -> bool) -> bool {
        let holds = |holder: &Site, id: u32| {
            holder.contents.atoms(false).any(|a| a == Atom::Fresh(id))
                || holder.untracked.contains(&Atom::Fresh(id))
        };
        let mut seen: SmallVec<[u32; 16]> = SmallVec::new();
        let mut pending: SmallVec<[u32; 16]> = SmallVec::new();
        pending.push(target);
        while let Some(id) = pending.pop() {
            if seen.contains(&id) {
                continue;
            }
            seen.push(id);
            let Some(site) = self.state.site(id) else {
                return true;
            };
            if site.held {
                return true;
            }
            for (holder, other) in self.state.sites.iter().enumerate() {
                let holder = holder as u32;
                if !seen.contains(&holder) && !shield(holder) && holds(other, id) {
                    pending.push(holder);
                }
            }
        }
        false
    }
}
