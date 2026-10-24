//! Replaying a callee summary at a call site: parameters become the caller's argument values and
//! each of the callee's allocation groups becomes one caller site.

use super::summary::{MAX_GROUPS, Summary, havoc};
use super::value::{Atom, Atoms, PathId, Slot, insert, one, union};
use super::walker::Walker;
use dream_types::TypeId;
use smallvec::SmallVec;

/// Site part reserved for objects allocated by a call no summary describes; each part names
/// one site per [`super::summary::FreshGroup`].
const HAVOC_PART: u32 = u32::MAX / MAX_GROUPS as u32 - 1;

/// The caller sites standing for each of the callee's allocation groups.
type Fresh<'f> = &'f [Option<Atom>];

/// A summary translated into the caller's atoms, in the summary's own order.
#[derive(Default)]
struct Mapped {
    contents: SmallVec<[(Atom, Option<Slot>, Atoms); 8]>,
    ret: Atoms,
    writes: SmallVec<[(Atoms, Atoms, Atoms); 8]>,
}

/// An entry of [`Mapped`] that a translation round added objects to.
#[derive(Clone, Copy)]
enum Grown {
    Contents(usize),
    Ret,
    Write(usize),
}

impl Walker<'_, '_> {
    pub(super) fn instantiate(
        &mut self,
        summary: &Summary,
        args: &[Atoms],
        key: usize,
        part: u32,
        refs: &[bool],
    ) -> Atoms {
        let mut held: SmallVec<[Atoms; 4]> = args.iter().cloned().collect();
        let mut fresh: SmallVec<[Option<Atom>; MAX_GROUPS]> = SmallVec::new();
        for (index, group) in summary.fresh.iter().enumerate() {
            let mut site = None;
            if group.allocates {
                let at = part * MAX_GROUPS as u32 + index as u32;
                let atom = self.site_holding(key, at, &mut held);
                if let Atom::Fresh(id) = atom {
                    let site = self.state.site_mut(id);
                    site.set_types(group.typed, &group.types);
                    site.held |= group.stray;
                    site.stray |= group.stray;
                }
                site = Some(atom);
            }
            fresh.push(site);
        }
        let fresh = &fresh[..];
        let args = &held[..];
        for (arg, &is_ref) in args.iter().zip(refs) {
            if is_ref {
                self.store(&[Atom::Old], None, arg, None);
            }
        }
        for (group, &site) in summary.fresh.iter().zip(fresh) {
            if let Some(site) = site.filter(|_| group.lost) {
                self.lose(&[site]);
            }
        }
        // A summary path names whatever object was there when the callee used it, which may
        // have been put there by one of its own earlier stores (in a loop, by a later one).
        // Writes replay in the callee's order; a write is replayed again whenever re-translating
        // it against the effects of the others finds more objects, until none does. No store
        // cuts a path until the replay is complete.
        let mut mapped = Mapped::default();
        loop {
            let grown = self.map_summary(summary, args, refs, fresh, &mut mapped);
            if grown.is_empty() {
                break;
            }
            for &at in &grown {
                match at {
                    Grown::Contents(index) => {
                        let (site, slot, atoms) = mapped.contents[index].clone();
                        self.store(&[site], slot, &atoms, None);
                    }
                    Grown::Write(index) => {
                        let write = &summary.writes[index];
                        let (targets, value, check) = mapped.writes[index].clone();
                        if targets.is_empty() {
                            continue;
                        }
                        let origin = write.origin.filter(|o| {
                            !write.safe && self.prog.graph.relevant(o.container, o.value)
                        });
                        if write.retained {
                            self.retain(&value);
                            continue;
                        }
                        if origin.is_some() {
                            self.check(&targets, &check, origin);
                        }
                        if write.stores {
                            self.store(&targets, write.slot, &value, None);
                        }
                    }
                    Grown::Ret => {}
                }
            }
        }
        let slots = self.map_slots(summary, args, refs, fresh);
        if summary.all_slots {
            self.wrote_all();
        } else {
            for (slot, targets) in slots {
                if !targets.is_empty() {
                    self.wrote(slot, &targets);
                }
            }
        }
        mapped.ret
    }

    /// Translates every summary atom against the current state into `mapped`, keeping what
    /// earlier rounds found. Returns the entries that gained objects, in summary order.
    fn map_summary(
        &mut self,
        summary: &Summary,
        args: &[Atoms],
        refs: &[bool],
        fresh: Fresh<'_>,
        mapped: &mut Mapped,
    ) -> SmallVec<[Grown; 8]> {
        let mut grown = SmallVec::new();
        let mut at = 0;
        for (group, &site) in summary.fresh.iter().zip(fresh) {
            let Some(site) = site else { continue };
            for (slot, atoms) in group.contents.buckets() {
                let atoms = self.map_all(atoms, args, refs, fresh);
                let grew = match mapped.contents.get_mut(at) {
                    Some((_, _, known)) => union(known, &atoms),
                    None => {
                        mapped.contents.push((site, slot, atoms));
                        true
                    }
                };
                if grew {
                    grown.push(Grown::Contents(at));
                }
                at += 1;
            }
        }
        let ret = self.map_all(&summary.ret, args, refs, fresh);
        if union(&mut mapped.ret, &ret) {
            grown.push(Grown::Ret);
        }
        for (index, write) in summary.writes.iter().enumerate() {
            let targets = self.map_all(&[write.target], args, refs, fresh);
            let value = self.map_all(&write.value, args, refs, fresh);
            let check = self.map_all(&write.check, args, refs, fresh);
            let grew = match mapped.writes.get_mut(index) {
                Some((known_targets, known_value, known_check)) => {
                    union(known_targets, &targets)
                        | union(known_value, &value)
                        | union(known_check, &check)
                }
                None => {
                    mapped.writes.push((targets, value, check));
                    true
                }
            };
            if grew {
                grown.push(Grown::Write(index));
            }
        }
        grown
    }

    fn map_slots(
        &mut self,
        summary: &Summary,
        args: &[Atoms],
        refs: &[bool],
        fresh: Fresh<'_>,
    ) -> SmallVec<[(Slot, Atoms); 8]> {
        summary
            .slots
            .iter()
            .map(|(&slot, targets)| (slot, self.map_all(targets, args, refs, fresh)))
            .collect()
    }

    /// A call nothing is known about: any argument may end up linked to any other.
    pub(super) fn havoc(&mut self, args: &[(Atoms, TypeId)], ret: TypeId, key: usize) -> Atoms {
        let params: SmallVec<[(u32, TypeId); 4]> = args
            .iter()
            .enumerate()
            .filter(|(_, (_, ty))| self.bearing(*ty))
            .map(|(i, (_, ty))| (i as u32, *ty))
            .collect();
        let summary = havoc(&params, self.bearing(ret));
        let atoms: SmallVec<[Atoms; 4]> = args.iter().map(|(a, _)| a.clone()).collect();
        self.instantiate(&summary, &atoms, key, HAVOC_PART, &[])
    }

    fn map_all(
        &mut self,
        atoms: &[Atom],
        args: &[Atoms],
        refs: &[bool],
        fresh: Fresh<'_>,
    ) -> Atoms {
        let mut out = Atoms::new();
        for &atom in atoms {
            let mapped = self.map(atom, args, refs, fresh);
            union(&mut out, &mapped);
        }
        out
    }

    fn map(&mut self, atom: Atom, args: &[Atoms], refs: &[bool], fresh: Fresh<'_>) -> Atoms {
        match atom {
            Atom::Old => one(Atom::Old),
            Atom::Fresh(group) => one(
                fresh
                    .get(group as usize)
                    .copied()
                    .flatten()
                    .unwrap_or(Atom::Old),
            ),
            Atom::Param(i, path) => {
                let i = i as usize;
                if refs.get(i).copied().unwrap_or(false) {
                    return one(Atom::Old);
                }
                let arg = match args.get(i) {
                    Some(arg) => arg.clone(),
                    None => {
                        // A closure body called directly reads whatever environment is current.
                        let mut env = self.closure_env.clone();
                        insert(&mut env, Atom::Old);
                        env
                    }
                };
                let mut out = Atoms::new();
                for base in arg {
                    let extended = self.extend(base, path);
                    union(&mut out, &extended);
                }
                out
            }
        }
    }

    /// The objects at `path` beneath `base`.
    pub(super) fn extend(&mut self, base: Atom, path: PathId) -> Atoms {
        if path == PathId::ROOT {
            return one(base);
        }
        match base {
            Atom::Old => one(Atom::Old),
            Atom::Param(root, prefix) => {
                if prefix.is_detached() && path.is_beneath() {
                    one(Atom::Param(root, PathId::BENEATH))
                } else if prefix.is_detached() && !self.paths.has_weak(path) {
                    // Everything an object detached from `root` owns is detached from it too.
                    one(Atom::Param(root, PathId::DETACHED))
                } else if prefix.is_detached()
                    || prefix == PathId::ANY
                    || prefix.is_beneath()
                    || path == PathId::ANY
                {
                    one(Atom::Param(root, PathId::ANY))
                } else if path == PathId::DETACHED || path == PathId::BENEATH {
                    one(Atom::Param(root, path))
                } else if path.is_cut() || path.is_linked() {
                    self.detached_at(base, path)
                } else {
                    self.follow(base, path)
                }
            }
            Atom::Fresh(id) => {
                if path == PathId::ANY {
                    let reach = self.state.reach(&[base], true);
                    let mut out = reach.others;
                    for id in reach.fresh {
                        insert(&mut out, Atom::Fresh(id));
                    }
                    out
                } else if path == PathId::DETACHED || path == PathId::BENEATH {
                    let mut contents: Atoms = self
                        .state
                        .site(id)
                        .map(|site| site.contents.atoms(false).collect())
                        .unwrap_or_default();
                    union(&mut contents, self.state.untracked_contents(id));
                    let reach = self.state.reach(&contents, false);
                    let mut out = reach.others;
                    for id in reach.fresh {
                        insert(&mut out, Atom::Fresh(id));
                    }
                    out
                } else if path.is_cut() || path.is_linked() {
                    self.detached_at(base, path)
                } else {
                    self.follow(base, path)
                }
            }
        }
    }

    /// The objects a callee found at the entry path of `path` beneath `base` before it cut
    /// that path. Ones the caller names by a parameter path have left that path for it too.
    fn detached_at(&mut self, base: Atom, path: PathId) -> Atoms {
        let mut out = Atoms::new();
        for atom in self.follow(base, path.entry()) {
            insert(
                &mut out,
                match atom {
                    Atom::Param(i, p) if p.is_exact() && p != PathId::ROOT => {
                        Atom::Param(i, if path.is_cut() { p.cut() } else { p.linked() })
                    }
                    other => other,
                },
            );
        }
        out
    }

    fn follow(&mut self, base: Atom, path: PathId) -> Atoms {
        let mut current = one(base);
        for slot in self.paths.slots(path) {
            current = self.read(&current, slot);
        }
        current
    }
}
