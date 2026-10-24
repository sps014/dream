//! Per-program-point facts: local values and what each allocation site may point to.

use super::contents::Contents;
use super::type_graph::TypeGraph;
use super::value::{Atom, Atoms, PathId, Paths, Slot, insert, union};
use dream_types::TypeId;
use smallvec::SmallVec;

#[derive(Clone, Default, PartialEq, Debug)]
pub(super) struct Site {
    /// Everything ever stored into objects of this site through its own atom.
    pub(super) contents: Contents,
    /// Reachable through storage this analysis does not follow, so its `contents` are incomplete
    /// and pointers to it may exist anywhere.
    pub(super) escaped: bool,
    /// Escaped to code no summary replays (globals, hosts, unknown objects), as opposed to only
    /// into a parameter's graph, where the caller re-tracks it from the recorded writes.
    pub(super) lost: bool,
    /// May be pointed to directly by something no site's contents track: an old object, a
    /// parameter's graph, a host, a global, or another task. Otherwise every holder is a site
    /// that has this one in its contents.
    pub(super) held: bool,
    /// `held` by something no recorded write names (another task, or an unknown a loop widened
    /// to), so callers cannot learn it by replaying writes.
    pub(super) stray: bool,
    /// Parameters whose own object holds this one directly. Objects detached from those
    /// parameters cannot reach it unless it is `poisoned`.
    pub(super) roots: SmallVec<[u32; 2]>,
    /// Parameters whose graph may hold it at any depth. Unless `lost` or `stray`, no handle
    /// derived from another parameter can denote its objects.
    pub(super) under: SmallVec<[u32; 2]>,
    /// Something detached from its roots may reach it: it was stored below a parameter's own
    /// object, a value that may alias it was stored, or a path that may lead to it was cut.
    pub(super) poisoned: bool,
    /// An object of this site exists on some path to this point.
    pub(super) live: bool,
    /// Everything stored through a parameter or old-object handle since this site escaped, which
    /// may have landed in its objects without passing through its own atom. Only code running
    /// during this function can store into its allocations, so this completes `contents`.
    pub(super) untracked: Atoms,
    /// The types its objects may have, when `typed`; otherwise any type.
    pub(super) types: SmallVec<[TypeId; 2]>,
    pub(super) typed: bool,
    /// Slots it was stored through into a parameter's graph: an access path from a parameter
    /// reaches its objects only by crossing one of them, unless `entered_anywhere`.
    pub(super) entries: SmallVec<[Slot; 2]>,
    pub(super) entered_anywhere: bool,
}

impl Site {
    pub(super) fn set_types(&mut self, typed: bool, types: &[TypeId]) {
        self.typed = typed;
        self.types = types.iter().copied().collect();
    }

    /// Folds the possible types of `other`'s objects into this site's.
    fn merge_types(&mut self, other: &Site) {
        if !other.live {
            return;
        }
        if !self.live {
            self.set_types(other.typed, &other.types);
            return;
        }
        self.typed &= other.typed;
        for &ty in &other.types {
            if !self.types.contains(&ty) {
                self.types.push(ty);
            }
        }
        self.types.sort_unstable();
    }

    fn merge_entries(&mut self, entries: &[Slot], anywhere: bool) {
        self.entered_anywhere |= anywhere;
        for &slot in entries {
            if !self.entries.contains(&slot) {
                self.entries.push(slot);
            }
        }
        self.entries.sort_unstable();
    }

    /// Whether a store into `slot` (`None`: any slot) of objects `targets` may write into this
    /// site's objects.
    pub(super) fn may_hold_store(
        &self,
        graph: &TypeGraph,
        paths: &Paths,
        slot: Option<Slot>,
        targets: &[Atom],
    ) -> bool {
        self.may_own(graph, slot) && targets.iter().any(|&target| self.may_be_at(paths, target))
    }

    /// Whether this site's objects may contain `slot` (`None`: any slot).
    pub(super) fn may_own(&self, graph: &TypeGraph, slot: Option<Slot>) -> bool {
        !self.typed || slot.is_none_or(|slot| graph.owns(&self.types, slot))
    }

    /// Whether a handle `target` other than this site's own atom may denote one of its objects.
    pub(super) fn may_be_at(&self, paths: &Paths, target: Atom) -> bool {
        if !self.escaped {
            return false;
        }
        match target {
            Atom::Fresh(_) => false,
            Atom::Param(root, _) if !self.lost && !self.stray && !self.under.contains(&root) => {
                false
            }
            Atom::Param(_, path)
                if (path.is_exact() || path.is_cut() || path.is_linked())
                    && !self.lost
                    && !self.entered_anywhere =>
            {
                self.entries.iter().any(|&slot| paths.contains(path.entry(), slot))
            }
            _ => true,
        }
    }

    pub(super) fn merge_under(&mut self, under: &[u32]) {
        for &root in under {
            if !self.under.contains(&root) {
                self.under.push(root);
            }
        }
        self.under.sort_unstable();
    }

    /// Escaped only into its roots' own objects, so detached values provably cannot reach it.
    pub(super) fn published(&self) -> bool {
        self.escaped && !self.lost && !self.poisoned
    }
}

/// A field whose current content is known exactly: `target.slot` holds `value`.
#[derive(Clone, PartialEq, Debug)]
pub(super) struct Known {
    pub(super) target: Atom,
    pub(super) slot: Slot,
    pub(super) value: Atoms,
}

#[derive(Clone, Default, PartialEq, Debug)]
pub(super) struct State {
    pub(super) locals: Vec<Atoms>,
    pub(super) sites: Vec<Site>,
    /// Sorted by `(target, slot)`.
    pub(super) known: Vec<Known>,
}

/// Objects a value may reach: tracked allocation sites plus everything else, flattened.
#[derive(Default)]
pub(super) struct Reach {
    pub(super) fresh: SmallVec<[u32; 8]>,
    pub(super) others: Atoms,
}

/// How a store's targets hold the stored value, for marking it escaped.
#[derive(Default)]
pub(super) struct Holder {
    pub(super) lost: bool,
    pub(super) roots: SmallVec<[u32; 2]>,
    pub(super) under: SmallVec<[u32; 2]>,
    pub(super) stray: bool,
    pub(super) poisoned: bool,
    /// Slots every access path from a parameter to the stored value crosses one of.
    pub(super) entries: SmallVec<[Slot; 2]>,
    pub(super) entered_anywhere: bool,
}

impl State {
    pub(super) fn local(&self, index: usize) -> Atoms {
        self.locals.get(index).cloned().unwrap_or_default()
    }

    pub(super) fn set_local(&mut self, index: usize, atoms: Atoms) {
        if index >= self.locals.len() {
            self.locals.resize(index + 1, Atoms::new());
        }
        self.locals[index] = atoms;
    }

    pub(super) fn site(&self, id: u32) -> Option<&Site> {
        self.sites.get(id as usize)
    }

    pub(super) fn site_mut(&mut self, id: u32) -> &mut Site {
        let index = id as usize;
        if index >= self.sites.len() {
            self.sites.resize(index + 1, Site::default());
        }
        &mut self.sites[index]
    }

    pub(super) fn escaped(&self, id: u32) -> bool {
        self.site(id).is_some_and(|s| s.escaped)
    }

    pub(super) fn known(&self, target: Atom, slot: Slot) -> Option<&Atoms> {
        self.known
            .binary_search_by(|k| (k.target, k.slot).cmp(&(target, slot)))
            .ok()
            .map(|at| &self.known[at].value)
    }

    pub(super) fn set_known(&mut self, target: Atom, slot: Slot, value: Atoms) {
        let entry = Known {
            target,
            slot,
            value,
        };
        match self
            .known
            .binary_search_by(|k| (k.target, k.slot).cmp(&(target, slot)))
        {
            Ok(at) => self.known[at] = entry,
            Err(at) => self.known.insert(at, entry),
        }
    }

    pub(super) fn join(&mut self, other: &State) {
        if other.locals.len() > self.locals.len() {
            self.locals.resize(other.locals.len(), Atoms::new());
        }
        for (mine, theirs) in self.locals.iter_mut().zip(&other.locals) {
            union(mine, theirs);
        }
        if other.sites.len() > self.sites.len() {
            self.sites.resize(other.sites.len(), Site::default());
        }
        for (mine, theirs) in self.sites.iter_mut().zip(&other.sites) {
            mine.merge_types(theirs);
            mine.merge_entries(&theirs.entries, theirs.entered_anywhere);
            mine.contents.merge(&theirs.contents);
            mine.escaped |= theirs.escaped;
            mine.lost |= theirs.lost;
            mine.held |= theirs.held;
            mine.stray |= theirs.stray;
            mine.poisoned |= theirs.poisoned;
            mine.live |= theirs.live;
            for &root in &theirs.roots {
                if !mine.roots.contains(&root) {
                    mine.roots.push(root);
                }
            }
            mine.roots.sort_unstable();
            mine.merge_under(&theirs.under);
            union(&mut mine.untracked, &theirs.untracked);
        }
        self.known.retain_mut(|k| match other.known(k.target, k.slot) {
            Some(value) => {
                union(&mut k.value, value);
                true
            }
            None => false,
        });
    }

    fn for_each_set(&mut self, mut f: impl FnMut(&mut Atoms)) {
        for set in &mut self.locals {
            f(set);
        }
        for site in &mut self.sites {
            for set in site.contents.sets_mut() {
                f(set);
            }
            f(&mut site.untracked);
        }
        for known in &mut self.known {
            f(&mut known.value);
        }
    }

    /// What an object of site `id` may hold beyond its tracked `contents`.
    pub(super) fn untracked_contents(&self, id: u32) -> &[Atom] {
        self.site(id).map_or(&[], |s| &s.untracked)
    }

    /// A store of `value` into `slot` (`None`: any slot) through `targets`, handles that may
    /// alias escaped objects.
    pub(super) fn alias(
        &mut self,
        graph: &TypeGraph,
        paths: &Paths,
        slot: Option<Slot>,
        targets: &[Atom],
        value: &[Atom],
    ) {
        for site in &mut self.sites {
            if site.escaped && site.may_hold_store(graph, paths, slot, targets) {
                union(&mut site.untracked, value);
            }
        }
    }

    /// Starts a new most-recent object at `mr`, folding the previous one into `old`. Keeping the
    /// newest allocation distinct is what lets `n.next = prev` in a loop prove `n` is not `prev`.
    pub(super) fn allocate(&mut self, mr: u32, old: u32) {
        if self.site(mr).is_some_and(|s| s.live) {
            self.for_each_set(|set| {
                if let Ok(at) = set.binary_search(&Atom::Fresh(mr)) {
                    set.remove(at);
                    insert(set, Atom::Fresh(old));
                }
            });
            let previous = std::mem::take(self.site_mut(mr));
            let target = self.site_mut(old);
            target.merge_types(&previous);
            target.merge_entries(&previous.entries, previous.entered_anywhere);
            target.contents.merge(&previous.contents);
            union(&mut target.untracked, &previous.untracked);
            target.escaped |= previous.escaped;
            target.lost |= previous.lost;
            target.held |= previous.held;
            target.stray |= previous.stray;
            target.merge_under(&previous.under);
            // The older generation is many objects; nothing tracks which root holds which.
            target.poisoned |= previous.escaped || target.escaped;
            target.live = true;
        }
        *self.site_mut(mr) = Site {
            live: true,
            ..Site::default()
        };
    }

    /// What `atoms` may reach through owning references, or with `weak` through any reference.
    pub(super) fn reach(&self, atoms: &[Atom], weak: bool) -> Reach {
        let mut reach = Reach::default();
        let mut pending: SmallVec<[Atom; 8]> = atoms.iter().copied().collect();
        while let Some(atom) = pending.pop() {
            match atom {
                Atom::Fresh(id) => {
                    if reach.fresh.contains(&id) {
                        continue;
                    }
                    reach.fresh.push(id);
                    if let Some(site) = self.site(id) {
                        pending.extend(site.contents.atoms(weak));
                    }
                    pending.extend(self.untracked_contents(id).iter().copied());
                }
                other => {
                    insert(&mut reach.others, other);
                }
            }
        }
        reach
    }

    /// Marks every allocation reachable from `atoms` as held by `holder`.
    pub(super) fn escape(&mut self, atoms: &[Atom], holder: &Holder) {
        let mut pending = fresh_ids(atoms);
        let mut reached: SmallVec<[u32; 8]> = SmallVec::new();
        while let Some(id) = pending.pop() {
            if reached.contains(&id) {
                continue;
            }
            reached.push(id);
            let site = self.site_mut(id);
            site.escaped = true;
            site.lost |= holder.lost;
            site.stray |= holder.stray;
            site.poisoned |= holder.poisoned;
            for &root in &holder.roots {
                if !site.roots.contains(&root) {
                    site.roots.push(root);
                }
            }
            site.roots.sort_unstable();
            site.merge_under(&holder.under);
            site.merge_entries(&holder.entries, holder.entered_anywhere);
            pending.extend(fresh_ids(&site.contents.flat()));
        }
    }

    /// A store through `slot` may cut any access path that crosses it. Objects beneath a cut
    /// become detached; published objects of the affected parameters might be among them,
    /// except `exempt` ones published by the cutting store itself.
    pub(super) fn demote(&mut self, paths: &Paths, slot: Slot, exempt: &[u32]) {
        let mut cut: SmallVec<[u32; 4]> = SmallVec::new();
        self.for_each_set(|set| demote_set(set, &mut cut, paths, |p| paths.contains(p, slot)));
        self.known.retain(|k| {
            k.slot != slot && !matches!(k.target, Atom::Param(_, p) if paths.contains(p, slot))
        });
        for (id, site) in self.sites.iter_mut().enumerate() {
            if !exempt.contains(&(id as u32)) && site.roots.iter().any(|r| cut.contains(r)) {
                site.poisoned = true;
            }
        }
    }

    pub(super) fn demote_all(&mut self, paths: &Paths) {
        let mut cut: SmallVec<[u32; 4]> = SmallVec::new();
        self.for_each_set(|set| demote_set(set, &mut cut, paths, |p| p != PathId::ROOT));
        self.known.clear();
        for site in &mut self.sites {
            site.poisoned |= site.escaped;
        }
    }

    /// A store whose value may reach parameter `root` may connect objects detached from it
    /// back to it.
    pub(super) fn reattach(&mut self, root: impl Fn(u32) -> bool) {
        self.for_each_set(|set| {
            let hit = |a: &Atom| matches!(a, Atom::Param(i, p) if p.is_detached() && root(*i));
            if set.iter().any(hit) {
                let old = std::mem::take(set);
                for atom in old {
                    insert(
                        set,
                        match atom {
                            Atom::Param(i, p) if p.is_cut() && root(i) => Atom::Param(i, p.linked()),
                            Atom::Param(i, p) if p.is_detached() && root(i) => {
                                Atom::Param(i, PathId::BENEATH)
                            }
                            other => other,
                        },
                    );
                }
            }
        });
    }

    /// Loop widening: every value may also be an arbitrary old object and nothing stays private.
    pub(super) fn widen(&mut self, paths: &Paths) {
        self.demote_all(paths);
        self.reattach(|_| true);
        for set in &mut self.locals {
            if !set.is_empty() {
                insert(set, Atom::Old);
            }
        }
        for site in &mut self.sites {
            site.escaped = true;
            site.lost = true;
            site.held = true;
            site.stray = true;
            insert(&mut site.untracked, Atom::Old);
        }
    }
}

fn fresh_ids(atoms: &[Atom]) -> SmallVec<[u32; 8]> {
    atoms
        .iter()
        .filter_map(|a| match a {
            Atom::Fresh(id) => Some(*id),
            _ => None,
        })
        .collect()
}

fn demote_set(
    set: &mut Atoms,
    cut: &mut SmallVec<[u32; 4]>,
    paths: &Paths,
    hit: impl Fn(PathId) -> bool,
) {
    let hits = |p: PathId| p.is_exact() && p != PathId::ROOT && hit(p);
    if !set
        .iter()
        .any(|a| matches!(a, Atom::Param(_, p) if hits(*p)))
    {
        return;
    }
    let old = std::mem::take(set);
    for atom in old {
        let atom = match atom {
            Atom::Param(i, p) if hits(p) => {
                if !cut.contains(&i) {
                    cut.push(i);
                }
                // Only owned objects stay beneath the parameter once the path to them is cut.
                if paths.has_weak(p) {
                    Atom::Param(i, PathId::ANY)
                } else {
                    Atom::Param(i, p.cut())
                }
            }
            other => other,
        };
        insert(set, atom);
    }
}
