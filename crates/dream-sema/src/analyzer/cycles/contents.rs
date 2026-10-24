//! What the objects of one allocation site may hold, kept apart per storage slot so reading one
//! field does not return what was stored into another.

use super::value::{Atom, Atoms, Slot, union};

/// Buckets sorted by slot. The `None` bucket holds values stored into a slot this analysis does
/// not name (tuple and union payloads, closure environments, callee allocations' summaries); it
/// answers every read.
#[derive(Clone, Default, PartialEq, Debug)]
pub(super) struct Contents(Vec<(Option<Slot>, Atoms)>);

impl Contents {
    pub(super) fn add(&mut self, slot: Option<Slot>, atoms: &[Atom]) {
        if atoms.is_empty() {
            return;
        }
        match self.0.binary_search_by(|(s, _)| s.cmp(&slot)) {
            Ok(at) => {
                union(&mut self.0[at].1, atoms);
            }
            Err(at) => self.0.insert(at, (slot, atoms.iter().copied().collect())),
        }
    }

    /// Everything a read of `slot` may return.
    pub(super) fn get(&self, slot: Slot, out: &mut Atoms) {
        for (s, atoms) in &self.0 {
            if s.is_none() || *s == Some(slot) {
                union(out, atoms);
            }
        }
    }

    pub(super) fn merge(&mut self, other: &Contents) {
        for (slot, atoms) in &other.0 {
            self.add(*slot, atoms);
        }
    }

    pub(super) fn buckets(&self) -> impl Iterator<Item = (Option<Slot>, &Atoms)> {
        self.0.iter().map(|(slot, atoms)| (*slot, atoms))
    }

    /// Everything held, or with `weak` false only what is held through owning slots.
    pub(super) fn atoms(&self, weak: bool) -> impl Iterator<Item = Atom> + '_ {
        self.0
            .iter()
            .filter(move |(slot, _)| weak || slot.is_none_or(Slot::owns))
            .flat_map(|(_, atoms)| atoms.iter().copied())
    }

    pub(super) fn sets_mut(&mut self) -> impl Iterator<Item = &mut Atoms> {
        self.0.iter_mut().map(|(_, atoms)| atoms)
    }

    pub(super) fn flat(&self) -> Atoms {
        let mut out = Atoms::new();
        for (_, atoms) in &self.0 {
            union(&mut out, atoms);
        }
        out
    }
}
