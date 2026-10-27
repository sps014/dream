//! Dependency-driven closure of function, fresh-constructor and interface effects.

use super::local::{Edge, LocalSummary};
use super::{ModRef, ModRefTable};
use crate::Mir;
use crate::analysis::function_key::FunctionKeyRef;
use dream_types::DefId;
use indexmap::IndexMap;
use std::collections::VecDeque;

enum Node {
    Function(usize),
    Constructor(DefId, Vec<usize>),
    Interface(usize, usize, Option<Vec<usize>>),
}

impl ModRefTable {
    pub(super) fn close_effects(&mut self, mir: &Mir, locals: &[LocalSummary]) -> usize {
        let mut nodes: Vec<Node> = (0..mir.functions.len()).map(Node::Function).collect();
        let functions: IndexMap<_, _> = mir
            .functions
            .iter()
            .enumerate()
            .map(|(index, f)| ((f.def, f.instance.as_slice()), index))
            .collect();
        let mut by_def: IndexMap<DefId, Vec<usize>> = IndexMap::new();
        for (index, f) in mir.functions.iter().enumerate() {
            by_def.entry(f.def).or_default().push(index);
        }
        let mut constructors = IndexMap::new();
        for (def, instances) in &by_def {
            constructors.insert(*def, nodes.len());
            nodes.push(Node::Constructor(*def, instances.clone()));
        }
        // A slot joins every concrete instance: the erased dispatch does not carry arguments.
        let mut targets: IndexMap<(usize, usize), Option<Vec<usize>>> = IndexMap::new();
        for implementation in &mir.interfaces.impls {
            for (id, slots) in &implementation.entries {
                for (slot, def) in slots.iter().enumerate() {
                    let entry = targets.entry((*id, slot)).or_insert_with(|| Some(vec![]));
                    match def.and_then(|def| by_def.get(&def)) {
                        Some(instances) => {
                            if let Some(targets) = entry {
                                targets.extend(instances.iter().copied());
                            }
                        }
                        None => *entry = None,
                    }
                }
            }
        }
        for local in locals {
            for edge in &local.edges {
                if let Edge::Iface(id, slot, _) = edge {
                    targets.entry((*id, *slot)).or_insert(None);
                }
            }
        }
        let mut interfaces = IndexMap::new();
        for ((id, slot), mut targets) in targets {
            if let Some(targets) = &mut targets {
                targets.sort_unstable();
                targets.dedup();
            }
            self.ifaces.insert((id, slot), ModRef::default());
            interfaces.insert((id, slot), nodes.len());
            nodes.push(Node::Interface(id, slot, targets));
        }

        let mut users = vec![vec![]; nodes.len()];
        for (index, node) in nodes.iter().enumerate() {
            let mut depend_on = |edge: &Edge| {
                let target = match edge {
                    Edge::Call(def, args, _) => functions.get(&(*def, args.as_slice())),
                    Edge::Ctor(def) => constructors.get(def),
                    Edge::Iface(id, slot, _) => interfaces.get(&(*id, *slot)),
                };
                if let Some(&target) = target {
                    users[target].push(index);
                }
            };
            match node {
                Node::Function(f) => {
                    for edge in &locals[*f].edges {
                        depend_on(edge);
                    }
                }
                Node::Constructor(_, instances) => {
                    for &f in instances {
                        for edge in &locals[f].edges {
                            depend_on(edge);
                        }
                    }
                }
                Node::Interface(_, _, Some(targets)) => {
                    for &target in targets {
                        users[target].push(index);
                    }
                }
                Node::Interface(_, _, None) => {}
            }
        }
        for users in &mut users {
            users.sort_unstable();
            users.dedup();
        }
        let mut queued = vec![true; nodes.len()];
        let mut queue: VecDeque<_> = (0..nodes.len()).collect();
        let mut evaluations = 0;
        let mut updates = 0;
        let mut ownership = vec![(false, false); nodes.len()];
        while let Some(index) = queue.pop_front() {
            evaluations += 1;
            queued[index] = false;
            let join = |a: (bool, bool), b: (bool, bool)| (a.0 || b.0, a.1 || b.1);
            let ownership_edge = |edge: &Edge| {
                let target = match edge {
                    Edge::Call(def, args, _) => functions.get(&(*def, args.as_slice())),
                    Edge::Ctor(def) => constructors.get(def),
                    Edge::Iface(id, slot, _) => interfaces.get(&(*id, *slot)),
                };
                target.map_or_else(
                    || match edge {
                        Edge::Call(def, args, _) => match self.call_def(*def, args) {
                            ModRef::Known(k) => (k.observes_ownership(), k.may_destroy),
                            _ => (true, true),
                        },
                        _ => (true, true),
                    },
                    |&target| ownership[target],
                )
            };
            let ownership_body = |f: usize| {
                let own = (
                    locals[f].observes_rc
                        || matches!(&locals[f].own, ModRef::Known(k) if k.observes_ownership()),
                    locals[f].may_destroy,
                );
                locals[f].edges.iter().map(ownership_edge).fold(own, join)
            };
            let next = match &nodes[index] {
                Node::Function(f) => ownership_body(*f),
                Node::Constructor(_, fs) => fs
                    .iter()
                    .map(|&f| ownership_body(f))
                    .fold((false, false), join),
                Node::Interface(_, _, targets) => {
                    targets.as_ref().map_or((true, true), |targets| {
                        targets
                            .iter()
                            .map(|&target| ownership[target])
                            .fold((false, false), join)
                    })
                }
            };
            let next = join(ownership[index], next);
            let observation_changed = next != ownership[index];
            ownership[index] = next;
            let changed = match &nodes[index] {
                Node::Function(f) => {
                    let summary = self.summarize(&locals[*f], false);
                    let f = &mir.functions[*f];
                    self.fns
                        .get_mut(&FunctionKeyRef {
                            def: f.def,
                            args: &f.instance,
                        })
                        .expect("function summary exists")
                        .join(&summary)
                }
                Node::Constructor(def, instances) => {
                    let mut summary = ModRef::default();
                    for &f in instances {
                        summary.join(&self.summarize(&locals[f], true));
                    }
                    self.ctors
                        .get_mut(def)
                        .expect("constructor summary exists")
                        .join(&summary)
                }
                Node::Interface(id, slot, targets) => {
                    let mut summary = ModRef::default();
                    if let Some(targets) = targets {
                        for &target in targets {
                            let f = &mir.functions[target];
                            summary.join(&self.call_def(f.def, &f.instance));
                        }
                    } else {
                        summary = ModRef::Top;
                    }
                    self.ifaces
                        .get_mut(&(*id, *slot))
                        .expect("interface summary exists")
                        .join(&summary)
                }
            };
            if changed || observation_changed {
                updates += 1;
                for &user in &users[index] {
                    if !queued[user] {
                        queued[user] = true;
                        queue.push_back(user);
                    }
                }
            }
        }
        self.observers = mir
            .functions
            .iter()
            .enumerate()
            .map(|(index, f)| ((f.def, f.instance.clone()), ownership[index].0))
            .collect();
        self.cleanup = mir
            .functions
            .iter()
            .enumerate()
            .map(|(index, f)| ((f.def, f.instance.clone()), ownership[index].1))
            .collect();
        tracing::info!(
            nodes = nodes.len(),
            evaluations,
            updates,
            "closed heap effects"
        );
        evaluations
    }

    fn summarize(&self, local: &LocalSummary, fresh: bool) -> ModRef {
        let mut summary = if fresh {
            local.own_fresh.clone()
        } else {
            local.own.clone()
        };
        for edge in &local.edges {
            summary.join(&match edge {
                Edge::Call(def, args, parameters) => self.call_def(*def, args).mapped(parameters),
                Edge::Ctor(def) => self.ctor(*def).mapped(&[]),
                Edge::Iface(id, slot, parameters) => self.iface(*id, *slot).mapped(parameters),
            });
        }
        summary
    }
}
