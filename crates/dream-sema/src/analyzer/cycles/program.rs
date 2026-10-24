//! Whole-program driver: call graph, bottom-up summaries, and entry-point reporting.

use super::summary::{Finding, Summary, havoc};
use super::type_graph::TypeGraph;
use super::value::Paths;
use super::visit;
use super::walker::Walker;
use dream_hir::{Binding, HExprKind, HFunction, HGlobal, InterfaceTable};
use dream_types::{DefId, TypeId};
use indexmap::{IndexMap, IndexSet};
use petgraph::graph::{DiGraph, NodeIndex};

/// Rounds a recursive call group gets to stabilize with precise summaries.
const MAX_ROUNDS: usize = 8;
/// A group still unstable after [`MAX_ROUNDS`] retries with coarse summaries (see
/// [`Summary::coarsen`]) for this many rounds before falling back to [`havoc`].
const MAX_COARSE_ROUNDS: usize = 32;

/// Bodiless functions with known effects.
#[derive(Default)]
pub(super) struct Builtins {
    /// The `Closure` intrinsics that build and unpack `fun` values.
    pub(super) funcbox_new: Option<DefId>,
    pub(super) funcbox_env: Option<DefId>,
    pub(super) funcbox_funcidx: Option<DefId>,
    /// The `Future` class an `async` function's call returns.
    pub(super) future: Option<DefId>,
    /// Intrinsics that only inspect their arguments: they can neither keep nor store a reference.
    pub(super) inspecting: IndexSet<DefId>,
    /// Intrinsics that return a new object and keep no reference to their arguments.
    pub(super) allocating: IndexSet<DefId>,
    /// Every compiler-implemented function; none of them calls back into Dream code.
    pub(super) intrinsics: IndexSet<DefId>,
}

pub(super) struct Program<'a> {
    pub(super) graph: TypeGraph<'a>,
    pub(super) functions: &'a [HFunction],
    pub(super) interfaces: &'a InterfaceTable,
    pub(super) builtins: Builtins,
    /// Classes marked `@allow_cycle`: the program breaks cycles through their objects itself.
    pub(super) managed: IndexSet<DefId>,
    by_key: IndexMap<(DefId, Vec<TypeId>), usize>,
    by_def: IndexMap<DefId, Vec<usize>>,
    boxed: IndexMap<TypeId, Vec<usize>>,
}

struct Scan {
    callees: IndexSet<usize>,
    needs_walk: bool,
}

impl<'a> Program<'a> {
    pub(super) fn new(
        graph: TypeGraph<'a>,
        functions: &'a [HFunction],
        interfaces: &'a InterfaceTable,
        builtins: Builtins,
        managed: IndexSet<DefId>,
    ) -> Self {
        let mut by_key = IndexMap::new();
        let mut by_def: IndexMap<DefId, Vec<usize>> = IndexMap::new();
        for (index, f) in functions.iter().enumerate() {
            by_key.insert((f.def, f.instance.clone()), index);
            by_def.entry(f.def).or_default().push(index);
        }
        Self {
            graph,
            functions,
            interfaces,
            builtins,
            managed,
            by_key,
            by_def,
            boxed: IndexMap::new(),
        }
    }

    pub(super) fn lookup(&self, def: DefId, instance: &[TypeId]) -> Option<usize> {
        if let Some(&index) = self.by_key.get(&(def, instance.to_vec())) {
            return Some(index);
        }
        if !instance.is_empty()
            && let Some(&index) = self.by_key.get(&(def, Vec::new()))
        {
            return Some(index);
        }
        match self.by_def.get(&def).map(Vec::as_slice) {
            Some([only]) => Some(*only),
            _ => None,
        }
    }

    /// Implementations reachable through an interface slot; `None` when generated glue fills a
    /// slot, which this analysis does not see.
    pub(super) fn interface_targets(&self, iface: usize, slot: usize) -> Option<Vec<usize>> {
        let mut out = Vec::new();
        for imp in &self.interfaces.impls {
            for (id, defs) in &imp.entries {
                if *id != iface {
                    continue;
                }
                let def = (*defs.get(slot)?)?;
                out.extend(self.by_def.get(&def)?.iter().copied());
            }
        }
        Some(out)
    }

    /// Every function ever boxed into a value of the `fun` type `sig`. Only `Closure.funcbox_new`
    /// over a Dream function makes `fun` values, and `fun` types are assignable only to themselves
    /// and `object`, so a call through `sig` reaches no other function; with no such box it is
    /// unreachable.
    pub(super) fn indirect_targets(&self, sig: TypeId) -> &[usize] {
        self.boxed.get(&sig).map_or(&[], Vec::as_slice)
    }

    fn scan(&mut self) -> Vec<Scan> {
        let mut pending_ifaces = Vec::new();
        let mut pending_sigs = Vec::new();
        let mut scans = Vec::with_capacity(self.functions.len());
        let mut boxed = Vec::new();
        for f in self.functions {
            let mut scan = Scan {
                callees: IndexSet::new(),
                needs_walk: f.params.iter().any(|p| self.graph.bearing(p.ty)),
            };
            let mut ifaces = Vec::new();
            let mut sigs = Vec::new();
            visit::stmts(&f.body, &mut |e| {
                scan.needs_walk |= self.graph.bearing(e.ty);
                match &e.kind {
                    HExprKind::Call { callee, args } => {
                        if Some(callee.def) == self.builtins.funcbox_new
                            && let Some(HExprKind::Var(Binding::Func(raw))) =
                                args.first().map(|a| &a.kind)
                            && let Some(index) = self.lookup(raw.def, &raw.instance)
                        {
                            boxed.push((e.ty, index));
                        }
                        if let Some(index) = self.lookup(callee.def, &callee.instance) {
                            scan.callees.insert(index);
                        }
                    }
                    HExprKind::MethodCall { callee, .. } => {
                        if let Some(index) = self.lookup(callee.def, &callee.instance) {
                            scan.callees.insert(index);
                        }
                    }
                    HExprKind::New {
                        ctor: Some(ctor),
                        instance,
                        ..
                    } => {
                        if let Some(index) = self.lookup(*ctor, instance) {
                            scan.callees.insert(index);
                        }
                    }
                    HExprKind::Var(Binding::Func(callee)) => {
                        if let Some(index) = self.lookup(callee.def, &callee.instance) {
                            scan.callees.insert(index);
                        }
                    }
                    HExprKind::InterfaceCall {
                        iface_id,
                        method_slot,
                        ..
                    } => ifaces.push((*iface_id, *method_slot)),
                    HExprKind::IndirectCall { sig, .. } => sigs.push(*sig),
                    _ => {}
                }
            });
            pending_ifaces.push(ifaces);
            pending_sigs.push(sigs);
            scans.push(scan);
        }
        for (ty, index) in boxed {
            let entry = self.boxed.entry(ty).or_default();
            if !entry.contains(&index) {
                entry.push(index);
            }
        }
        for (scan, (ifaces, sigs)) in scans.iter_mut().zip(pending_ifaces.into_iter().zip(pending_sigs)) {
            for (iface, slot) in ifaces {
                scan.callees
                    .extend(self.interface_targets(iface, slot).unwrap_or_default());
            }
            for sig in sigs {
                scan.callees
                    .extend(self.indirect_targets(sig).iter().copied());
            }
        }
        scans
    }

    pub(super) fn havoc_for(&self, index: usize) -> Summary {
        let f = &self.functions[index];
        let env = (f.params.len() as u32, self.graph.interner().object());
        let params: Vec<_> = f
            .params
            .iter()
            .enumerate()
            .filter(|(_, p)| self.graph.bearing(p.ty))
            .map(|(i, p)| (i as u32, p.ty))
            .chain(std::iter::once(env))
            .collect();
        havoc(&params, self.graph.bearing(f.ret) || f.is_async)
    }
}

pub(super) fn run(
    program: &mut Program<'_>,
    globals: &[HGlobal],
    exports: &IndexSet<DefId>,
) -> Vec<Finding> {
    let scans = program.scan();
    let program = &*program;
    let count = program.functions.len();
    let mut calls: DiGraph<(), ()> = DiGraph::with_capacity(count, count);
    for _ in 0..count {
        calls.add_node(());
    }
    for (caller, scan) in scans.iter().enumerate() {
        for &callee in &scan.callees {
            calls.add_edge(NodeIndex::new(caller), NodeIndex::new(callee), ());
        }
    }
    let reachable = reachable(program, &scans, exports);
    let mut paths = Paths::new();
    let mut summaries = vec![Summary::default(); count];
    let mut findings: Vec<Vec<Finding>> = vec![Vec::new(); count];
    // Tarjan yields callees before callers, so each group sees final summaries of its callees.
    for group in petgraph::algo::tarjan_scc(&calls) {
        let members: Vec<usize> = group
            .iter()
            .map(|n| n.index())
            .filter(|&i| scans[i].needs_walk && reachable[i])
            .collect();
        if members.is_empty() {
            continue;
        }
        let recursive = group.len() > 1 || scans[members[0]].callees.contains(&members[0]);
        let mut stable = !recursive;
        let phases: &[(bool, usize)] = if recursive {
            &[(false, MAX_ROUNDS), (true, MAX_COARSE_ROUNDS)]
        } else {
            &[(false, 1)]
        };
        'phases: for &(coarse, rounds) in phases {
            if coarse {
                // Precise summaries are not a coarse fixpoint's lower bound; start over.
                for &m in &members {
                    summaries[m] = Summary::default();
                }
            }
            for _ in 0..rounds {
                let mut changed = false;
                for &m in &members {
                    let entry = exports.contains(&program.functions[m].def);
                    let (summary, found) =
                        Walker::function(program, &mut paths, &summaries, m, entry, coarse);
                    findings[m] = found;
                    if summary != summaries[m] {
                        summaries[m] = summary;
                        changed = true;
                    }
                }
                if !changed {
                    stable = true;
                    break 'phases;
                }
            }
        }
        if !stable {
            for &m in &members {
                summaries[m] = program.havoc_for(m);
            }
            for &m in &members {
                let entry = exports.contains(&program.functions[m].def);
                findings[m] = Walker::function(program, &mut paths, &summaries, m, entry, true).1;
            }
        }
    }
    let mut out: IndexSet<Finding> = findings.into_iter().flatten().collect();
    out.extend(Walker::globals(program, &mut paths, &summaries, globals));
    out.into_iter().collect()
}

/// Functions user code can run. Library code nothing calls is not the program's concern; user
/// functions, synthesized entry points and exports are always roots.
fn reachable(program: &Program<'_>, scans: &[Scan], exports: &IndexSet<DefId>) -> Vec<bool> {
    let mut seen = vec![false; scans.len()];
    let mut pending: Vec<usize> = program
        .functions
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            !f.file.as_deref().is_some_and(dream_stdlib::is_std_source) || exports.contains(&f.def)
        })
        .map(|(i, _)| i)
        .collect();
    while let Some(index) = pending.pop() {
        if std::mem::replace(&mut seen[index], true) {
            continue;
        }
        pending.extend(scans[index].callees.iter().copied().filter(|&c| !seen[c]));
    }
    seen
}
