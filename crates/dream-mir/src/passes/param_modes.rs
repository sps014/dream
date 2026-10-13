//! Parameter-specific borrowing proofs run before ownership insertion. A closed synchronous
//! call graph without observable cleanup can pass a nonescaping parameter at +0; returned
//! aliases and ownership transfers keep their +1 independently of other parameters.

mod proof;

use crate::analysis::function_key::FunctionKeyRef;
use crate::{Callee, Mir, Rvalue, Statement};
use dream_types::TypeInterner;
use indexmap::{IndexMap, IndexSet};

pub(crate) const STAGE: &str = "param-modes";
type Key = (dream_types::DefId, Vec<dream_types::TypeId>);

fn key(callee: &Callee) -> FunctionKeyRef<'_> {
    FunctionKeyRef {
        def: callee.def,
        args: &callee.args,
    }
}

pub(crate) fn run(mir: &mut Mir, interner: &TypeInterner) -> bool {
    if mir.profile.is_debug() {
        return false;
    }
    let proofs: IndexMap<Key, _> = mir
        .functions
        .iter()
        .map(|f| {
            (
                (f.def, f.instance.clone()),
                proof::Proof::of(f, interner, &mir.layouts),
            )
        })
        .collect();
    // NewCtor has no concrete instance key and omits the receiver in its parameter flags.
    // Its ABI cannot be changed by a proof keyed on a concrete function instance.
    let constructors: IndexSet<_> = mir
        .functions
        .iter()
        .chain(&mir.polls)
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.stmts)
        .filter_map(|s| match s {
            Statement::Assign(_, Rvalue::New { ctor: Some(c), .. }) => Some(c.def),
            _ => None,
        })
        .collect();
    // Reject unsafe components before changing any ABI, including recursive backedges.
    let mut closed: IndexSet<_> = proofs
        .iter()
        .filter(|(_, p)| p.safe)
        .map(|(k, _)| k.clone())
        .collect();
    loop {
        let rejected: Vec<_> = proofs
            .iter()
            .filter(|(k, p)| {
                closed.contains(*k) && p.calls.iter().any(|c| !closed.contains(&key(&c.callee)))
            })
            .map(|(k, _)| k.clone())
            .collect();
        if rejected.is_empty() {
            break;
        }
        for k in rejected {
            closed.shift_remove(&k);
        }
    }
    let mut borrowed: IndexMap<Key, Vec<bool>> = proofs
        .iter()
        .map(|(k, p)| {
            let fixed_abi =
                constructors.contains(&k.0) || mir.exports.iter().any(|(def, _)| *def == k.0);
            (
                k.clone(),
                p.parameters
                    .iter()
                    .map(|&yes| yes && closed.contains(k) && !fixed_abi)
                    .collect(),
            )
        })
        .collect();
    loop {
        let mut rejected = Vec::new();
        for (k, p) in &proofs {
            for (parameter, &yes) in borrowed[k].iter().enumerate() {
                if yes
                    && p.calls.iter().any(|call| {
                        call.arguments.iter().enumerate().any(|(index, roots)| {
                            roots.contains(&parameter)
                                && call.callee.take_params.get(index).copied().unwrap_or(true)
                                && !borrowed
                                    .get(&key(&call.callee))
                                    .and_then(|v| v.get(index))
                                    .copied()
                                    .unwrap_or(false)
                        })
                    })
                {
                    rejected.push((k.clone(), parameter));
                }
            }
        }
        if rejected.is_empty() {
            break;
        }
        for (k, parameter) in rejected {
            borrowed.get_mut(&k).expect("proof key")[parameter] = false;
        }
    }
    let mut modes = IndexMap::new();
    let mut changed = false;
    for f in &mut mir.functions {
        let k = (f.def, f.instance.clone());
        for (index, p) in f.params.iter().enumerate() {
            let d = &mut f.locals[p.0 as usize];
            if d.is_take && borrowed[&k][index] {
                d.is_take = false;
                changed = true;
            }
        }
        modes.insert(
            k,
            f.params
                .iter()
                .map(|p| f.locals[p.0 as usize].is_take)
                .collect::<Vec<_>>(),
        );
    }
    for f in mir.functions.iter_mut().chain(&mut mir.polls) {
        for stmt in f.blocks.iter_mut().flat_map(|b| &mut b.stmts) {
            let callee = match stmt {
                Statement::Call { callee, .. }
                | Statement::Assign(_, Rvalue::Call { callee, .. }) => callee,
                _ => continue,
            };
            if let Some(takes) = modes.get(&key(callee)) {
                changed |= callee.take_params != *takes;
                callee.take_params.clone_from(takes);
            }
        }
    }
    changed
}
