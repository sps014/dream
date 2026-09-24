//! Closed-world interface devirtualization. An [`Statement::InterfaceCall`] /
//! [`Rvalue::InterfaceCall`] becomes a direct call the inliner can eat when either
//!
//! - every implementor maps the slot to one method symbol, or
//! - the receiver's runtime class is known exactly at the call: a flow-sensitive forward dataflow
//!   tracks locals whose every reaching definition is an [`Rvalue::New`] of one class (directly or
//!   through `Use` / reference `Cast` / `Move` chains). Runs between inliner rounds, so allocations
//!   exposed by inlining a factory are seen too.
//!
//! Receivers with up to four statically known implementors still dispatch through the itable in
//! MIR; the C backend emits a tag switch to direct calls for those (`backend/c/iface_guard.rs`).

use super::cfg::reverse_postorder;
use super::ModulePass;
use crate::{Callee, Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{TypeId, TypeInterner};
use indexmap::IndexMap;

pub struct Devirt;

impl ModulePass for Devirt {
    fn name(&self) -> &'static str {
        "devirt"
    }

    fn run(&self, mir: &mut crate::Mir, interner: &TypeInterner) -> bool {
        let targets = Targets::build(mir);
        if targets.is_empty() {
            return false;
        }
        let mut changed = false;
        for f in &mut mir.functions {
            changed |= devirt_function(f, &targets, interner);
        }
        changed
    }
}

type SlotKey = (usize, usize);

/// Direct-call targets derived from the interface table.
#[derive(Default)]
struct Targets {
    /// `(iface, slot)` whose every implementor supplies the same method.
    unique: IndexMap<SlotKey, Callee>,
    /// `(class, iface, slot)` → that class's method.
    by_class: IndexMap<(TypeId, usize, usize), Callee>,
}

impl Targets {
    fn build(mir: &crate::Mir) -> Self {
        let by_name: IndexMap<&str, &MirFunction> =
            mir.functions.iter().map(|f| (f.name.as_str(), f)).collect();
        let mut names: IndexMap<SlotKey, Option<&str>> = IndexMap::new();
        let mut out = Targets::default();
        for imp in &mir.interfaces.impls {
            for (iface_id, slots) in &imp.entries {
                for (slot, sym) in slots.iter().enumerate() {
                    let key = (*iface_id, slot);
                    names
                        .entry(key)
                        .and_modify(|prev| {
                            if *prev != Some(sym.as_str()) {
                                *prev = None;
                            }
                        })
                        .or_insert(Some(sym.as_str()));
                    if let Some(f) = by_name.get(sym.as_str()) {
                        out.by_class
                            .insert((imp.class_ty, *iface_id, slot), callee_of(f));
                    }
                }
            }
        }
        for (key, name) in names {
            if let Some(f) = name.and_then(|n| by_name.get(n)) {
                out.unique.insert(key, callee_of(f));
            }
        }
        out
    }

    fn is_empty(&self) -> bool {
        self.unique.is_empty() && self.by_class.is_empty()
    }

    fn lookup(&self, key: SlotKey, exact: Option<TypeId>) -> Option<&Callee> {
        self.unique
            .get(&key)
            .or_else(|| exact.and_then(|ty| self.by_class.get(&(ty, key.0, key.1))))
    }
}

fn callee_of(f: &MirFunction) -> Callee {
    Callee {
        def: f.def,
        args: f.instance.clone(),
        ret: f.ret,
        take_params: f
            .params
            .iter()
            .map(|p| f.locals[p.0 as usize].is_take)
            .collect(),
    }
}

fn devirt_function(f: &mut MirFunction, targets: &Targets, interner: &TypeInterner) -> bool {
    let has_iface = f.blocks.iter().any(|b| {
        b.stmts.iter().any(|s| {
            matches!(
                s,
                Statement::InterfaceCall { .. }
                    | Statement::Assign(_, Rvalue::InterfaceCall { .. })
            )
        })
    });
    if !has_iface {
        return false;
    }
    let facts = ExactFacts::solve(f, interner);
    let mut changed = false;
    for (bi, block) in f.blocks.iter_mut().enumerate() {
        let mut state = facts.entry[bi]
            .clone()
            .unwrap_or_else(|| vec![None; facts.width]);
        for stmt in &mut block.stmts {
            let exact = receiver_local(stmt).and_then(|r| facts.get(&state, r));
            changed |= rewrite_stmt(stmt, targets, exact);
            facts.transfer(stmt, &mut state, interner);
        }
    }
    changed
}

fn receiver_local(stmt: &Statement) -> Option<Local> {
    match stmt {
        Statement::InterfaceCall {
            receiver: Operand::Copy(Place::Local(r)),
            ..
        }
        | Statement::Assign(
            _,
            Rvalue::InterfaceCall {
                receiver: Operand::Copy(Place::Local(r)),
                ..
            },
        ) => Some(*r),
        _ => None,
    }
}

/// Per-block entry state of "local holds an object whose runtime class is exactly `ty`". Only
/// candidate locals (those that can transitively receive a heap `New`) get a slot, so the state
/// stays small in large functions.
struct ExactFacts {
    slot: Vec<Option<usize>>,
    width: usize,
    /// `None` = block unreachable from the entry.
    entry: Vec<Option<Vec<Option<TypeId>>>>,
}

impl ExactFacts {
    fn solve(f: &MirFunction, interner: &TypeInterner) -> Self {
        let slot = candidate_slots(f, interner);
        let width = slot.iter().flatten().count();
        let mut facts = ExactFacts {
            slot,
            width,
            entry: vec![None; f.blocks.len()],
        };
        facts.entry[f.entry.0 as usize] = Some(vec![None; width]);
        let rpo = reverse_postorder(f);
        loop {
            let mut changed = false;
            for &b in &rpo {
                let Some(mut state) = facts.entry[b.0 as usize].clone() else {
                    continue;
                };
                let block = f.block(b);
                for stmt in &block.stmts {
                    facts.transfer(stmt, &mut state, interner);
                }
                if let Terminator::Await { dest: Some(d), .. } = &block.terminator {
                    facts.set(&mut state, *d, None);
                }
                for succ in block.terminator.successors() {
                    changed |= meet_into(&mut facts.entry[succ.0 as usize], &state);
                }
            }
            if !changed {
                return facts;
            }
        }
    }

    fn get(&self, state: &[Option<TypeId>], l: Local) -> Option<TypeId> {
        self.slot
            .get(l.0 as usize)
            .copied()
            .flatten()
            .and_then(|s| state[s])
    }

    fn set(&self, state: &mut [Option<TypeId>], l: Local, v: Option<TypeId>) {
        if let Some(Some(s)) = self.slot.get(l.0 as usize) {
            state[*s] = v;
        }
    }

    fn transfer(&self, stmt: &Statement, state: &mut [Option<TypeId>], interner: &TypeInterner) {
        let Statement::Assign(Place::Local(d), rv) = stmt else {
            return;
        };
        let v = match rv {
            Rvalue::New { ty, .. } if !interner.is_value_type(*ty) => Some(*ty),
            Rvalue::Use(Operand::Copy(Place::Local(s))) | Rvalue::Move { src: s, .. } => {
                self.get(state, *s)
            }
            Rvalue::Cast(Operand::Copy(Place::Local(s)), _, to) if !interner.is_value_type(*to) => {
                self.get(state, *s)
            }
            _ => None,
        };
        self.set(state, *d, v);
    }
}

/// `true` when `into` lost information (or was first reached).
fn meet_into(into: &mut Option<Vec<Option<TypeId>>>, from: &[Option<TypeId>]) -> bool {
    match into {
        None => {
            *into = Some(from.to_vec());
            true
        }
        Some(cur) => {
            let mut changed = false;
            for (c, f) in cur.iter_mut().zip(from) {
                if c.is_some() && *c != *f {
                    *c = None;
                    changed = true;
                }
            }
            changed
        }
    }
}

/// Locals that can hold an exactly-typed heap object: targets of a reference `New`, closed over
/// `Use`/`Cast`/`Move` copies from other candidates.
fn candidate_slots(f: &MirFunction, interner: &TypeInterner) -> Vec<Option<usize>> {
    let mut is_cand = vec![false; f.locals.len()];
    loop {
        let mut changed = false;
        for stmt in f.blocks.iter().flat_map(|b| &b.stmts) {
            let Statement::Assign(Place::Local(d), rv) = stmt else {
                continue;
            };
            let src_cand = match rv {
                Rvalue::New { ty, .. } => !interner.is_value_type(*ty),
                Rvalue::Use(Operand::Copy(Place::Local(s)))
                | Rvalue::Move { src: s, .. }
                | Rvalue::Cast(Operand::Copy(Place::Local(s)), ..) => is_cand[s.0 as usize],
                _ => false,
            };
            if src_cand && !is_cand[d.0 as usize] {
                is_cand[d.0 as usize] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut next = 0;
    is_cand
        .into_iter()
        .map(|c| {
            c.then(|| {
                next += 1;
                next - 1
            })
        })
        .collect()
}

fn rewrite_stmt(stmt: &mut Statement, targets: &Targets, exact: Option<TypeId>) -> bool {
    match stmt {
        Statement::InterfaceCall {
            receiver,
            iface_id,
            method_slot,
            args,
            ..
        } => {
            let Some(callee) = targets.lookup((*iface_id, *method_slot), exact) else {
                return false;
            };
            let mut call_args = Vec::with_capacity(args.len() + 1);
            call_args.push(receiver.clone());
            call_args.extend(args.iter().cloned());
            *stmt = Statement::Call {
                callee: callee.clone(),
                args: call_args,
            };
            true
        }
        Statement::Assign(
            place,
            Rvalue::InterfaceCall {
                receiver,
                iface_id,
                method_slot,
                args,
                ret,
                ..
            },
        ) => {
            let Some(mut callee) = targets.lookup((*iface_id, *method_slot), exact).cloned() else {
                return false;
            };
            callee.ret = *ret;
            let mut call_args = Vec::with_capacity(args.len() + 1);
            call_args.push(receiver.clone());
            call_args.extend(args.iter().cloned());
            *stmt = Statement::Assign(
                place.clone(),
                Rvalue::Call {
                    callee,
                    args: call_args,
                },
            );
            true
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "devirt_tests.rs"]
mod tests;
