//! Reaching parameter dependencies. Killing a local definition must not kill aliases
//! already copied elsewhere, and joins union every predecessor's possible dependency.

use super::proof::sources;
use crate::{Mir, MirFunction, Place, Rvalue, Statement, Terminator};
use dream_hir::LayoutTable;
use dream_types::TypeInterner;
use indexmap::IndexSet;
use std::collections::VecDeque;

type State = Vec<IndexSet<usize>>;
pub(super) type ReturnSummaries = indexmap::IndexMap<super::Key, IndexSet<usize>>;

pub(super) fn returns(mir: &Mir, i: &TypeInterner) -> ReturnSummaries {
    let mut summaries: ReturnSummaries = mir
        .functions
        .iter()
        .map(|f| ((f.def, f.instance.clone()), IndexSet::new()))
        .collect();
    let indices: indexmap::IndexMap<_, _> = mir
        .functions
        .iter()
        .enumerate()
        .map(|(index, f)| ((f.def, f.instance.clone()), index))
        .collect();
    let mut users = vec![IndexSet::new(); mir.functions.len()];
    for (caller, f) in mir.functions.iter().enumerate() {
        for stmt in f.blocks.iter().flat_map(|b| &b.stmts) {
            if let Statement::Assign(_, Rvalue::Call { callee, .. }) = stmt
                && let Some(&callee) = indices.get(&super::key(callee))
            {
                users[callee].insert(caller);
            }
        }
    }
    let mut queue: VecDeque<_> = (0..mir.functions.len()).collect();
    let mut queued = vec![true; mir.functions.len()];
    let mut evaluations = 0;
    while let Some(index) = queue.pop_front() {
        evaluations += 1;
        queued[index] = false;
        let f = &mir.functions[index];
        if !i.is_reference(f.ret) && !i.is_value_type(f.ret) {
            continue;
        }
        let entries = compute(f, i, &mir.layouts, &summaries);
        let mut returned = IndexSet::new();
        for (bi, b) in f.blocks.iter().enumerate() {
            let mut state = entries[bi].clone();
            for stmt in &b.stmts {
                transfer(stmt, &mut state, f, i, &mir.layouts, &summaries);
            }
            if let Terminator::Return(Some(op)) = &b.terminator {
                returned.extend(super::proof::roots(op, &state, f, i, &mir.layouts));
            }
        }
        let current = summaries
            .get_mut(&(f.def, f.instance.clone()))
            .expect("seeded return summary");
        let before = current.len();
        current.extend(returned);
        if current.len() != before {
            for &caller in &users[index] {
                if !queued[caller] {
                    queued[caller] = true;
                    queue.push_back(caller);
                }
            }
        }
    }
    tracing::debug!(
        evaluations,
        functions = mir.functions.len(),
        "closed returned-parameter dependencies"
    );
    summaries
}

pub(super) fn compute(
    f: &MirFunction,
    i: &TypeInterner,
    layouts: &LayoutTable,
    returns: &ReturnSummaries,
) -> Vec<State> {
    let mut entries = vec![None; f.blocks.len()];
    let mut entry = vec![IndexSet::new(); f.locals.len()];
    for (index, p) in f.params.iter().enumerate() {
        entry[p.0 as usize].insert(index);
    }
    entries[f.entry.0 as usize] = Some(entry);
    let mut queue = VecDeque::from([f.entry.0 as usize]);
    let mut queued = vec![false; f.blocks.len()];
    queued[f.entry.0 as usize] = true;
    while let Some(bi) = queue.pop_front() {
        queued[bi] = false;
        let mut state = entries[bi].clone().expect("reachable entry");
        for stmt in &f.blocks[bi].stmts {
            transfer(stmt, &mut state, f, i, layouts, returns);
        }
        for next in f.blocks[bi].terminator.successors() {
            let next = next.0 as usize;
            let changed = match &mut entries[next] {
                None => {
                    entries[next] = Some(state.clone());
                    true
                }
                Some(entry) => {
                    let mut changed = false;
                    for (dst, src) in entry.iter_mut().zip(&state) {
                        let before = dst.len();
                        dst.extend(src);
                        changed |= dst.len() != before;
                    }
                    changed
                }
            };
            if changed && !queued[next] {
                queue.push_back(next);
                queued[next] = true;
            }
        }
    }
    entries
        .into_iter()
        .map(|entry| entry.unwrap_or_else(|| vec![(0..f.params.len()).collect(); f.locals.len()]))
        .collect()
}

fn transfer(
    stmt: &Statement,
    state: &mut State,
    f: &MirFunction,
    i: &TypeInterner,
    layouts: &LayoutTable,
    returns: &ReturnSummaries,
) {
    if let Statement::Assign(Place::Local(dst), rv) = stmt {
        state[dst.0 as usize] = sources(rv, state, f, i, layouts, returns);
    }
}
