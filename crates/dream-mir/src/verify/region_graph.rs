//! Flow-sensitive graph identities keep writes through aliases visible to other roots.

use crate::{Local, MirFunction, Operand, Place, Rvalue};
use std::collections::{BTreeMap, BTreeSet};

pub(super) type Origins = BTreeSet<usize>;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Root {
    Param(u32),
    Global(u32),
    Site(usize, usize),
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct Graph {
    pub locals: BTreeMap<u32, Origins>,
    roots: BTreeMap<u32, BTreeSet<Root>>,
    globals: BTreeMap<u32, BTreeSet<Root>>,
    depths: BTreeMap<Root, Origins>,
    external: BTreeSet<Root>,
}

impl Graph {
    pub fn clear(&mut self, place: &Place) {
        match place {
            Place::Local(l) => {
                self.locals.remove(&l.0);
                self.roots.remove(&l.0);
            }
            Place::Global(g) => {
                self.globals.insert(g.0, BTreeSet::new());
            }
            _ => {}
        }
    }

    pub fn new(f: &MirFunction, refs: &super::ref_types::RefTypes) -> Self {
        let mut graph = Self::default();
        for &local in &f.params {
            if refs.contains(f.local_ty(local)) {
                let root = Root::Param(local.0);
                graph.roots.insert(local.0, BTreeSet::from([root]));
                if !f.locals[local.0 as usize].is_cursor {
                    graph.external.insert(root);
                }
            }
        }
        graph
    }

    fn operand_roots(&self, op: &Operand) -> BTreeSet<Root> {
        match op {
            Operand::Copy(Place::Local(l))
            | Operand::Copy(Place::Field { base: l, .. })
            | Operand::Copy(Place::Index { base: l, .. })
            | Operand::Copy(Place::Deref { ptr: l, .. }) => {
                self.roots.get(&l.0).cloned().unwrap_or_default()
            }
            Operand::Copy(Place::Global(g)) => self
                .globals
                .get(&g.0)
                .cloned()
                .unwrap_or_else(|| BTreeSet::from([Root::Global(g.0)])),
            _ => BTreeSet::new(),
        }
    }

    pub fn operand(&self, op: &Operand) -> Origins {
        let mut origins: Origins = self
            .operand_roots(op)
            .iter()
            .flat_map(|r| self.depths.get(r).into_iter().flatten().copied())
            .collect();
        let mut locals = Vec::new();
        super::operands::operand_locals(op, &mut locals);
        for local in locals {
            origins.extend(self.locals.get(&local).into_iter().flatten());
        }
        origins
    }

    pub fn local(&self, local: u32) -> Origins {
        self.operand(&Operand::Copy(Place::Local(Local(local))))
    }

    pub fn write(&mut self, op: &Operand, origins: &Origins) {
        for root in self.operand_roots(op) {
            self.depths.entry(root).or_default().extend(origins);
        }
    }

    pub fn inherit(&mut self, place: &Place, operands: &[Operand]) {
        let roots: BTreeSet<_> = operands
            .iter()
            .flat_map(|op| self.operand_roots(op))
            .collect();
        match place {
            Place::Local(l) => self.roots.entry(l.0).or_default().extend(roots),
            Place::Global(g) => self.globals.entry(g.0).or_default().extend(roots),
            _ => {}
        }
    }

    pub fn define(
        &mut self,
        dest: &Place,
        rv: &Rvalue,
        origins: Origins,
        fresh: bool,
        bi: usize,
        si: usize,
    ) {
        let mut roots = match rv {
            Rvalue::Use(op) | Rvalue::Cast(op, _, _) | Rvalue::UnionField { base: op, .. } => {
                self.operand_roots(op)
            }
            Rvalue::Move { src, .. } => self.operand_roots(&Operand::Copy(Place::Local(*src))),
            _ => BTreeSet::new(),
        };
        if fresh || (!origins.is_empty() && roots.is_empty()) {
            let root = Root::Site(bi, si);
            self.depths.insert(root, origins.clone());
            roots.insert(root);
        }
        match dest {
            Place::Local(l) => {
                self.locals.insert(l.0, origins);
                self.roots.insert(l.0, roots);
            }
            Place::Global(g) => {
                self.globals.insert(g.0, roots);
            }
            _ => {}
        }
    }

    pub fn rewind(&mut self, depth: usize) {
        for origins in self.locals.values_mut().chain(self.depths.values_mut()) {
            if origins.remove(&depth) {
                origins.insert(0);
            }
        }
    }

    pub fn escapes(&self, depth: usize) -> bool {
        if self
            .depths
            .iter()
            .any(|(r, origins)| matches!(r, Root::Global(_)) && origins.contains(&depth))
        {
            return true;
        }
        self.external
            .iter()
            .chain(self.globals.values().flatten())
            .any(|r| self.depths.get(r).is_some_and(|o| o.contains(&depth)))
    }

    pub fn join(&mut self, other: &Self) -> bool {
        fn union<K: Ord + Copy, V: Ord + Copy>(
            dest: &mut BTreeMap<K, BTreeSet<V>>,
            src: &BTreeMap<K, BTreeSet<V>>,
        ) -> bool {
            let mut changed = false;
            for (&key, values) in src {
                let slot = dest.entry(key).or_default();
                let size = slot.len();
                slot.extend(values);
                changed |= slot.len() != size;
            }
            changed
        }
        let mut changed = union(&mut self.locals, &other.locals);
        changed |= union(&mut self.roots, &other.roots);
        changed |= union(&mut self.globals, &other.globals);
        changed | union(&mut self.depths, &other.depths)
    }
}
