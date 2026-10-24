//! The store check. A store `o.f = v` cannot close a cycle when `v` cannot reach `o`, which this
//! analysis proves in one of four ways:
//!
//! - `o` is a private allocation (nothing untracked points to it) that `v` does not reach.
//! - `v` was read from strictly beneath `o` along an access path no store has cut since.
//! - `o` is a parameter's own object and `v` is detached from that parameter: it was beneath the
//!   parameter until a store cut the path to it. Reaching the parameter again would need a later
//!   store whose value can reach the parameter, and every such store re-attaches what was
//!   detached (turns it back into "anywhere beneath the parameter").
//! - `o` was allocated here and stored only into a parameter's own object, and `v` is detached
//!   from that parameter. The only way to `o` is through the parameter, which detached values
//!   cannot reach. Storing a value that may alias `o` anywhere, or cutting a path that may lead
//!   to `o`, withdraws this.

use super::state::{Holder, Reach};
use super::summary::{Origin, Write};
use super::value::{Atom, Atoms, PathId, Slot};
use super::walker::Walker;
use smallvec::SmallVec;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Store,
    Check,
    Retain,
    Lose,
}

impl Walker<'_, '_> {
    /// Records a strong store of `value` into `slot` of objects `targets`. `origin` is `Some`
    /// when the store must be checked for closing a cycle.
    pub(super) fn store(
        &mut self,
        targets: &[Atom],
        slot: Option<Slot>,
        value: &[Atom],
        origin: Option<Origin>,
    ) {
        self.transfer(targets, slot, value, origin, Mode::Store);
    }

    /// Checks a store a callee proved safe only if `value` cannot reach `targets`, without
    /// storing anything.
    pub(super) fn check(&mut self, targets: &[Atom], value: &[Atom], origin: Option<Origin>) {
        self.transfer(targets, None, value, origin, Mode::Check);
    }

    /// Hands `value` to a holder outside every Dream object (a host or a global). It stays
    /// alive and may come back as an old object, but nothing is stored into any object.
    pub(super) fn retain(&mut self, value: &[Atom]) {
        self.transfer(&[Atom::Old], None, value, None, Mode::Retain);
    }

    /// A callee lost `value` to code no summary replays. Whatever holds it directly was
    /// recorded as its own write or marks it `stray`; this only accounts for the escape.
    pub(super) fn lose(&mut self, value: &[Atom]) {
        self.transfer(&[Atom::Old], None, value, None, Mode::Lose);
    }

    fn transfer(
        &mut self,
        targets: &[Atom],
        slot: Option<Slot>,
        value: &[Atom],
        origin: Option<Origin>,
        mode: Mode,
    ) {
        let stores = mode != Mode::Check;
        let fitting = self.fitting(slot, value);
        let value = fitting.as_slice();
        if value.is_empty() {
            return;
        }
        let fallback = [Atom::Old];
        let targets = if targets.is_empty() {
            &fallback[..]
        } else {
            targets
        };
        let owned = self.owning(slot, targets);
        let targets = owned.as_slice();
        if targets.is_empty() {
            return;
        }
        let mut holder = Holder::default();
        let mut untracked = false;
        if stores && mode != Mode::Lose && targets.iter().any(|t| !matches!(t, Atom::Fresh(_))) {
            for &atom in value {
                if let Atom::Fresh(id) = atom {
                    self.state.site_mut(id).held = true;
                }
            }
            if mode == Mode::Store {
                self.state
                    .alias(&self.prog.graph, self.paths, slot, targets, value);
            }
        }
        for &target in targets {
            match target {
                Atom::Fresh(id) => {
                    let site = self.state.site_mut(id);
                    if stores {
                        site.contents.add(slot, value);
                    }
                    if site.escaped {
                        untracked = true;
                        holder.lost |= site.lost;
                        holder.poisoned |= site.poisoned;
                        holder.entered_anywhere |= site.entered_anywhere;
                        for &entry in &site.entries {
                            if !holder.entries.contains(&entry) {
                                holder.entries.push(entry);
                            }
                        }
                        for &root in &site.roots {
                            if !holder.roots.contains(&root) {
                                holder.roots.push(root);
                            }
                        }
                        for &root in &site.under {
                            if !holder.under.contains(&root) {
                                holder.under.push(root);
                            }
                        }
                        holder.stray |= site.stray;
                    }
                }
                Atom::Param(root, path) => {
                    untracked = true;
                    if !holder.under.contains(&root) {
                        holder.under.push(root);
                    }
                    if path == PathId::ROOT {
                        if !holder.roots.contains(&root) {
                            holder.roots.push(root);
                        }
                    } else {
                        holder.poisoned = true;
                    }
                    match slot {
                        Some(slot) if !holder.entries.contains(&slot) => holder.entries.push(slot),
                        Some(_) => {}
                        None => holder.entered_anywhere = true,
                    }
                }
                Atom::Old => {
                    untracked = true;
                    holder.lost = true;
                    holder.entered_anywhere = true;
                }
            }
        }
        let reach = self.state.reach(value, false);
        let exported: Atoms = value.iter().copied().collect();
        for &target in targets {
            let private = matches!(target, Atom::Fresh(id) if !self.state.escaped(id));
            let mut deferred = false;
            let mut check = Atoms::new();
            if let Some(origin) = origin {
                let mut offending: SmallVec<[Atom; 4]> = SmallVec::new();
                let mut closes = false;
                if !private {
                    offending.extend(
                        reach
                            .others
                            .iter()
                            .copied()
                            .filter(|&b| !self.cannot_reach(b, target)),
                    );
                }
                let reached = matches!(target, Atom::Fresh(id) if reach.fresh.contains(&id));
                // A handle may denote an allocation the value reaches.
                let aliases = !matches!(target, Atom::Fresh(_))
                    && reach.fresh.iter().any(|&id| {
                        self.state.site(id).is_some_and(|s| {
                            s.may_hold_store(&self.prog.graph, self.paths, slot, &[target])
                        })
                    });
                let shielded = (reached || aliases || !offending.is_empty())
                    && self.managed_store(target, slot, value);
                if shielded {
                    offending.clear();
                }
                let direct = reached && !shielded;
                let aliases = aliases && !shielded;
                match target {
                    Atom::Param(..) if aliases => {
                        deferred = true;
                        check = exported.clone();
                    }
                    Atom::Old if aliases => closes = true,
                    _ => {}
                }
                if check.is_empty() {
                    check = offending.iter().copied().collect();
                }
                let roots = self.published_roots(target);
                for &b in &offending {
                    if target.is_param() || b.is_param() {
                        deferred = true;
                    } else if roots.is_empty() {
                        closes = true;
                    }
                }
                if !closes
                    && !direct
                    && !offending.is_empty()
                    && !roots.is_empty()
                    && !target.is_param()
                {
                    // Only the root parameters' own objects hold the target, so whatever reaches
                    // it reaches one of them first; each caller checks that against its
                    // arguments.
                    deferred = false;
                    for root in roots {
                        self.summary.record(Write {
                            target: Atom::Param(root, PathId::ROOT),
                            slot: None,
                            value: exported.clone(),
                            check: check.clone(),
                            stores: false,
                            retained: false,
                            safe: false,
                            origin: Some(origin),
                            line: self.line,
                        });
                    }
                }
                if closes || direct {
                    self.finding(origin);
                }
            }
            if !private && mode != Mode::Lose && (stores || deferred) {
                self.summary.record(Write {
                    target,
                    slot,
                    value: exported.clone(),
                    check: if deferred { check } else { Atoms::new() },
                    stores,
                    retained: mode == Mode::Retain,
                    safe: !deferred,
                    origin: if deferred { origin } else { None },
                    line: if deferred { self.line } else { 0 },
                });
            }
        }
        if stores && untracked {
            if mode == Mode::Store {
                self.reattach(&reach);
                self.poison(&reach);
            }
            // Only objects first published by this store are new to the paths it cuts; one
            // published earlier may still hang beneath the detached old value.
            let first: SmallVec<[u32; 8]> = reach
                .fresh
                .iter()
                .copied()
                .filter(|&id| !self.state.escaped(id))
                .collect();
            self.state.escape(value, &holder);
            if self.publishing {
                self.published_now.extend(first);
            }
        }
    }

    /// The objects of `value` that `slot`'s declared type can hold. Merged allocation sites may
    /// stand for objects of several types; only those of a fitting type are really stored.
    fn fitting(&self, slot: Option<Slot>, value: &[Atom]) -> Atoms {
        let Some(declared) = slot.and_then(|slot| self.prog.graph.slot_type(slot)) else {
            return value.iter().copied().collect();
        };
        value
            .iter()
            .copied()
            .filter(|&atom| match atom {
                Atom::Fresh(id) => self.state.site(id).is_none_or(|site| {
                    !site.typed
                        || site
                            .types
                            .iter()
                            .any(|&ty| self.prog.graph.may_hold(declared, ty))
                }),
                _ => true,
            })
            .collect()
    }

    /// The objects of `targets` that can have `slot`: only objects of the slot's owner type do,
    /// unless the owner is an inline value that other objects embed.
    fn owning(&self, slot: Option<Slot>, targets: &[Atom]) -> Atoms {
        targets
            .iter()
            .copied()
            .filter(|&atom| match atom {
                Atom::Fresh(id) => self
                    .state
                    .site(id)
                    .is_none_or(|site| site.may_own(&self.prog.graph, slot)),
                _ => true,
            })
            .collect()
    }

    /// The parameters whose own objects are the only holders of `target`.
    fn published_roots(&self, target: Atom) -> SmallVec<[u32; 2]> {
        match target {
            Atom::Fresh(id) => self
                .state
                .site(id)
                .filter(|s| s.published())
                .map(|s| s.roots.clone())
                .unwrap_or_default(),
            _ => SmallVec::new(),
        }
    }

    /// Whether the acyclic heap guarantees `value` cannot reach `target`.
    fn cannot_reach(&self, value: Atom, target: Atom) -> bool {
        match (target, value) {
            (Atom::Param(i, short), Atom::Param(j, long)) => {
                i == j && self.paths.strict_ancestor(short, long)
            }
            (Atom::Fresh(id), Atom::Param(root, path)) if path.is_detached() => self
                .state
                .site(id)
                .is_some_and(|s| s.published() && s.roots.contains(&root)),
            _ => false,
        }
    }

    /// A newly reachable value that may reach a parameter's object could connect objects detached
    /// from that parameter back to it.
    fn reattach(&mut self, reach: &Reach) {
        let mut spared: Option<u32> = None;
        let mut every = false;
        for &b in &reach.others {
            match b {
                Atom::Param(j, path)
                    if (path.is_exact() && path != PathId::ROOT && !self.paths.has_weak(path))
                        || path.is_detached() =>
                {
                    // Beneath (or detached from) its own parameter, but other parameters may
                    // alias anything.
                    match spared {
                        None => spared = Some(j),
                        Some(k) if k == j => {}
                        Some(_) => every = true,
                    }
                }
                _ => every = true,
            }
        }
        match (every, spared) {
            (true, _) => self.state.reattach(|_| true),
            (false, Some(j)) => self.state.reattach(|i| i != j),
            (false, None) => {}
        }
    }

    /// Published allocations stay unreachable from detached values only while no value that may
    /// alias them is stored anywhere. The value's own allocations are re-held through `escape`.
    fn poison(&mut self, reach: &Reach) {
        if reach.others.is_empty() {
            return;
        }
        for (id, site) in self.state.sites.iter_mut().enumerate() {
            if !site.published() || reach.fresh.contains(&(id as u32)) {
                continue;
            }
            let may_alias = reach.others.iter().any(|b| {
                !matches!(b, Atom::Param(root, path) if path.is_detached() && site.roots.contains(root))
            });
            if may_alias {
                site.poisoned = true;
            }
        }
    }
}
