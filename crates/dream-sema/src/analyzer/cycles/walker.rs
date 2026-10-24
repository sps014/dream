//! Flow-sensitive walk of one function body, producing its summary and unresolvable findings.
//!
//! Soundness rests on one inductive invariant: before each strong store the heap has no strong
//! cycle among cycle-capable types. A store `o.f = v` keeps that true when `v` cannot reach `o`.
//! The facts that prove it are in [`super::store`].

use super::program::Program;
use super::state::State;
use super::summary::{Finding, MAX_WRITES, Origin, PRECISE_WRITES, Summary};
use super::value::{
    Atom, Atoms, PathId, Paths, Slot, frame_slot, insert, one, result_slot, union,
};
use dream_hir::{HExpr, HFunction, HGlobal, ParamMode};
use dream_types::{TyKind, TypeId};
use indexmap::{IndexMap, IndexSet};
use smallvec::SmallVec;

pub(super) struct LoopFrame {
    pub(super) label: Option<String>,
    pub(super) breaks: Option<State>,
    pub(super) continues: Option<State>,
}

pub(super) struct Walker<'p, 'a> {
    pub(super) prog: &'p Program<'a>,
    pub(super) paths: &'p mut Paths,
    pub(super) summaries: &'p [Summary],
    pub(super) state: State,
    pub(super) summary: Summary,
    pub(super) loops: Vec<LoopFrame>,
    pub(super) exit: Option<State>,
    pub(super) ref_locals: IndexSet<u32>,
    /// The environment the next indirect call publishes to its callee.
    pub(super) closure_env: Atoms,
    /// Parameter index under which this function's own closure environment is summarized.
    env_index: Option<u32>,
    /// The body reads its closure environment; only closure bodies do.
    reads_env: bool,
    local_types: IndexMap<u32, TypeId>,
    pub(super) findings: IndexSet<Finding>,
    sites: IndexMap<(usize, u32), u32>,
    next_site: u32,
    pub(super) line: u32,
    pub(super) file: Option<&'a str>,
    /// Set while the current statement's own store runs, so the objects it publishes are not
    /// suspected of lying beneath the paths that same store cuts.
    pub(super) publishing: bool,
    pub(super) published_now: SmallVec<[u32; 8]>,
    /// Values of the enclosing expressions evaluated so far. They keep denoting the objects they
    /// did when a later operand allocates a site's next object.
    held: Vec<Atoms>,
}

impl<'p, 'a> Walker<'p, 'a> {
    fn new(
        prog: &'p Program<'a>,
        paths: &'p mut Paths,
        summaries: &'p [Summary],
        file: Option<&'a str>,
    ) -> Self {
        Self {
            prog,
            paths,
            summaries,
            state: State::default(),
            summary: Summary::default(),
            loops: Vec::new(),
            exit: None,
            ref_locals: IndexSet::new(),
            closure_env: Atoms::new(),
            env_index: None,
            reads_env: false,
            local_types: IndexMap::new(),
            findings: IndexSet::new(),
            sites: IndexMap::new(),
            next_site: 0,
            line: 0,
            file,
            publishing: false,
            published_now: SmallVec::new(),
            held: Vec::new(),
        }
    }

    pub(super) fn function(
        prog: &'p Program<'a>,
        paths: &'p mut Paths,
        summaries: &'p [Summary],
        index: usize,
        entry: bool,
        coarse: bool,
    ) -> (Summary, Vec<Finding>) {
        let f: &'a HFunction = &prog.functions[index];
        let mut w = Walker::new(prog, paths, summaries, f.file.as_deref());
        w.env_index = Some(f.params.len() as u32);
        w.local_types = f
            .locals
            .iter()
            .map(|l| (l.id.0, l.ty))
            .chain(f.params.iter().map(|p| (p.local.0, p.ty)))
            .collect();
        for (i, p) in f.params.iter().enumerate() {
            if p.mode == ParamMode::Ref {
                w.ref_locals.insert(p.local.0);
            }
            let atoms = if !prog.graph.bearing(p.ty) {
                Atoms::new()
            } else if p.mode == ParamMode::Ref {
                one(Atom::Old)
            } else {
                one(Atom::Param(i as u32, PathId::ROOT))
            };
            w.state.set_local(p.local.0 as usize, atoms);
        }
        w.block(&f.body);
        let end = std::mem::take(&mut w.state);
        let mut exit = w.exit.take().unwrap_or_default();
        exit.join(&end);
        if f.is_async {
            // The returned future owns the coroutine frame, which holds every argument, and
            // later holds the result.
            let result = std::mem::take(&mut w.summary.ret);
            let mut frame = Atoms::new();
            for (i, p) in f.params.iter().enumerate() {
                if prog.graph.bearing(p.ty) {
                    insert(&mut frame, Atom::Param(i as u32, PathId::ROOT));
                }
            }
            if w.reads_env {
                insert(&mut frame, Atom::Param(f.params.len() as u32, PathId::ROOT));
            }
            let future = exit.sites.len() as u32;
            let future_ty = prog.builtins.future.and_then(|def| {
                prog.graph
                    .interner()
                    .lookup(&TyKind::Struct(def, vec![f.ret]))
            });
            let site = exit.site_mut(future);
            site.live = true;
            match future_ty {
                Some(ty) => {
                    site.set_types(true, &[ty]);
                    site.contents.add(Some(frame_slot(ty)), &frame);
                    site.contents.add(Some(result_slot(ty)), &result);
                }
                None => {
                    site.contents.add(None, &frame);
                    site.contents.add(None, &result);
                }
            }
            w.summary.ret = one(Atom::Fresh(future));
        }
        let mut summary = std::mem::take(&mut w.summary).finish(&exit, coarse);
        let mut findings: Vec<Finding> = w.findings.into_iter().collect();
        if coarse || summary.writes.len() > PRECISE_WRITES {
            summary.coarsen();
        }
        if summary.writes.len() > MAX_WRITES {
            summary = prog.havoc_for(index);
        }
        if entry {
            findings.extend(
                summary
                    .writes
                    .iter()
                    .filter(|write| !write.safe)
                    .filter_map(|write| {
                        Some(Finding {
                            origin: write.origin?,
                            file: f.file.clone(),
                            line: write.line,
                        })
                    }),
            );
        }
        (summary, findings)
    }

    pub(super) fn globals(
        prog: &'p Program<'a>,
        paths: &'p mut Paths,
        summaries: &'p [Summary],
        globals: &[HGlobal],
    ) -> Vec<Finding> {
        let mut w = Walker::new(prog, paths, summaries, None);
        for global in globals {
            if let Some(init) = &global.init {
                let value = w.eval(init);
                w.retain(&value);
            }
        }
        w.findings.into_iter().collect()
    }

    pub(super) fn env_param(&mut self) -> Atoms {
        self.reads_env = true;
        one(match self.env_index {
            Some(index) => Atom::Param(index, PathId::ROOT),
            None => Atom::Old,
        })
    }

    pub(super) fn local_type(&self, local: u32) -> Option<TypeId> {
        self.local_types.get(&local).copied()
    }

    pub(super) fn bearing(&self, ty: TypeId) -> bool {
        self.prog.graph.bearing(ty)
    }

    /// The newest object allocated by expression `key`; `part` separates an expression's own
    /// allocation from the objects each possible callee allocates.
    pub(super) fn site(&mut self, key: usize, part: u32) -> Atom {
        let mr = match self.sites.get(&(key, part)) {
            Some(&id) => id,
            None => {
                let id = self.next_site;
                self.next_site += 2;
                self.sites.insert((key, part), id);
                id
            }
        };
        self.state.allocate(mr, mr + 1);
        age(&mut self.closure_env, mr);
        for atoms in &mut self.held {
            age(atoms, mr);
        }
        Atom::Fresh(mr)
    }

    /// Evaluates `e` while `held`, evaluated earlier, keeps denoting the objects it did.
    pub(super) fn eval_holding(&mut self, held: &mut Atoms, e: &HExpr) -> Atoms {
        self.held.push(std::mem::take(held));
        let out = self.eval(e);
        *held = self.held.pop().unwrap_or_default();
        out
    }

    /// Evaluates operands in order, each while the earlier ones are held.
    pub(super) fn eval_all<'e>(
        &mut self,
        exprs: impl Iterator<Item = &'e HExpr>,
    ) -> SmallVec<[Atoms; 4]> {
        let base = self.held.len();
        for e in exprs {
            let atoms = self.eval(e);
            self.held.push(atoms);
        }
        self.held.drain(base..).collect()
    }

    /// Records that the newest object of `site` has type `ty`.
    pub(super) fn typed(&mut self, site: Atom, ty: TypeId) -> Atom {
        if let Atom::Fresh(id) = site {
            self.state.site_mut(id).set_types(true, &[ty]);
        }
        site
    }

    /// Allocates like [`Self::site`] while `held` values, evaluated earlier, keep denoting the
    /// objects they did: the previous newest object of the site is now in its older generation.
    pub(super) fn site_holding(&mut self, key: usize, part: u32, held: &mut [Atoms]) -> Atom {
        let site = self.site(key, part);
        if let Atom::Fresh(mr) = site {
            for atoms in held {
                age(atoms, mr);
            }
        }
        site
    }

    pub(super) fn read(&mut self, base: &[Atom], slot: Slot) -> Atoms {
        let mut out = Atoms::new();
        for &atom in base {
            match atom {
                Atom::Old => {
                    insert(&mut out, Atom::Old);
                }
                Atom::Param(i, path) => match self.state.known(atom, slot) {
                    Some(known) => {
                        let known = known.clone();
                        union(&mut out, &known);
                    }
                    None => {
                        let p = self.paths.extend(path, slot);
                        insert(&mut out, Atom::Param(i, p));
                    }
                },
                Atom::Fresh(id) => {
                    if let Some(site) = self.state.site(id) {
                        site.contents.get(slot, &mut out);
                    }
                    union(&mut out, self.state.untracked_contents(id));
                }
            }
        }
        out
    }

    /// A store through `slot` of `targets` may have cut access paths crossing it. Objects private
    /// to this function lie on no access path, so writing only those cuts nothing.
    pub(super) fn wrote(&mut self, slot: Slot, targets: &[Atom]) {
        let mut visible = Atoms::new();
        for &target in targets {
            if !matches!(target, Atom::Fresh(id) if !self.state.escaped(id)) {
                insert(&mut visible, target);
            }
        }
        if targets.is_empty() {
            insert(&mut visible, Atom::Old);
        }
        if visible.is_empty() {
            return;
        }
        self.state.demote(self.paths, slot, &self.published_now);
        // Callers only ask whether a written object is private to them, so anything beneath a
        // parameter's own object is summarized as all of its graph, which keeps recursion finite.
        let mut exported = Atoms::new();
        for atom in visible {
            insert(
                &mut exported,
                match atom {
                    Atom::Param(i, path) if path != PathId::ROOT => Atom::Param(i, PathId::ANY),
                    other => other,
                },
            );
        }
        self.summary.wrote(slot, &exported);
    }

    pub(super) fn wrote_all(&mut self) {
        self.state.demote_all(self.paths);
        self.summary.wrote_all();
    }

    /// Other tasks run only while this one is suspended, and may store anything into objects
    /// they share with it. Private allocations stay private.
    pub(super) fn suspend(&mut self) {
        self.wrote_all();
        self.state.reattach(|_| true);
        self.state
            .alias(&self.prog.graph, self.paths, None, &[Atom::Old], &[Atom::Old]);
        for site in &mut self.state.sites {
            if site.escaped {
                site.lost = true;
                site.held = true;
                site.stray = true;
                site.poisoned = true;
            }
        }
    }

    pub(super) fn finding(&mut self, origin: Origin) {
        self.findings.insert(Finding {
            origin,
            file: self.file.map(str::to_string),
            line: self.line,
        });
    }
}

/// Moves `Fresh(mr)` in `atoms` to its older generation after site `mr` allocated again.
fn age(atoms: &mut Atoms, mr: u32) {
    if let Ok(at) = atoms.binary_search(&Atom::Fresh(mr)) {
        atoms.remove(at);
        insert(atoms, Atom::Fresh(mr + 1));
    }
}
