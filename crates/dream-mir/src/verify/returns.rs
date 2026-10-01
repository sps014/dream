//! Finite, module-wide return provenance. Recursive calls converge without an iteration cap.

use super::operands::rvalue_local_operands;
use crate::{Callee, Mir, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefId, TypeId, TypeInterner};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct Sources {
    pub fresh: bool,
    pub params: BTreeSet<usize>,
}

impl Sources {
    fn extend(&mut self, other: &Self) -> bool {
        let old = self.clone();
        self.fresh |= other.fresh;
        self.params.extend(&other.params);
        *self != old
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct Facts {
    pub result: Sources,
    pub writes: BTreeMap<usize, Sources>,
}

impl Facts {
    fn extend(&mut self, other: &Self) -> bool {
        let mut changed = self.result.extend(&other.result);
        for (&param, sources) in &other.writes {
            changed |= self.writes.entry(param).or_default().extend(sources);
        }
        changed
    }
}

pub(super) type Returns = BTreeMap<(DefId, Vec<TypeId>), Facts>;

pub(super) fn summarize(mir: &Mir, interner: &TypeInterner) -> Returns {
    let mut returns: Returns = mir
        .functions
        .iter()
        .map(|f| ((f.def, f.instance.clone()), Facts::default()))
        .collect();
    loop {
        let mut changed = false;
        for f in &mir.functions {
            let sources = function_sources(f, interner, &returns);
            changed |= returns
                .get_mut(&(f.def, f.instance.clone()))
                .expect("function summary")
                .extend(&sources);
        }
        if !changed {
            return returns;
        }
    }
}

pub(super) fn call_facts(callee: &Callee, arg_count: usize, returns: &Returns) -> Facts {
    returns
        .get(&(callee.def, callee.args.clone()))
        .cloned()
        .unwrap_or_else(|| {
            // Extern/indirect implementations can allocate in the active TLS region or forward any
            // argument. Missing bodies must not erase the caller's allocation provenance.
            let sources = Sources {
                fresh: true,
                params: (0..arg_count).collect(),
            };
            Facts {
                result: sources.clone(),
                writes: (0..arg_count).map(|p| (p, sources.clone())).collect(),
            }
        })
}

fn function_sources(f: &MirFunction, interner: &TypeInterner, returns: &Returns) -> Facts {
    if f.is_async {
        return Facts {
            result: Sources {
                fresh: true,
                params: (0..f.params.len()).collect(),
            },
            writes: BTreeMap::new(),
        };
    }
    let mut locals = vec![Sources::default(); f.locals.len()];
    for (index, local) in f.params.iter().enumerate() {
        if interner.is_rc_tracked(f.local_ty(*local)) {
            locals[local.0 as usize].params.insert(index);
        }
    }
    let mut result = Facts::default();
    loop {
        let mut changed = false;
        for block in &f.blocks {
            for stmt in &block.stmts {
                let opaque = match stmt {
                    Statement::Assign(
                        _,
                        rv @ (Rvalue::IndirectCall { .. }
                        | Rvalue::InterfaceCall { .. }
                        | Rvalue::JsCall { .. }),
                    ) => rvalue_local_operands(rv),
                    Statement::IndirectCall { .. }
                    | Statement::InterfaceCall { .. }
                    | Statement::JsCall { .. } => super::operands::other_stmt_locals(stmt),
                    _ => Vec::new(),
                };
                if !opaque.is_empty() {
                    let mut effect = Sources {
                        fresh: true,
                        params: BTreeSet::new(),
                    };
                    for &local in &opaque {
                        effect.extend(&locals[local as usize]);
                    }
                    for local in opaque {
                        if interner.is_rc_tracked(f.local_ty(crate::Local(local))) {
                            changed |= record_write(
                                crate::Local(local),
                                &effect,
                                &mut locals,
                                &mut result,
                            );
                        }
                    }
                }
                let direct_call = match stmt {
                    Statement::Call { callee, args }
                    | Statement::Assign(_, Rvalue::Call { callee, args }) => Some((callee, args)),
                    _ => None,
                };
                if let Some((callee, args)) = direct_call {
                    let facts = call_facts(callee, args.len(), returns);
                    let old = locals.clone();
                    for (index, sources) in facts.writes {
                        if let Some(arg) = args.get(index) {
                            if let Some(local) = argument_local(arg) {
                                let effect = substitute(&sources, args, &old);
                                changed |= record_write(local, &effect, &mut locals, &mut result);
                            }
                        }
                    }
                }
                if let Statement::Assign(place, rv) = stmt {
                    let dest = match place {
                        Place::Local(dest)
                        | Place::Field { base: dest, .. }
                        | Place::Index { base: dest, .. }
                        | Place::Deref { ptr: dest, .. } => dest,
                        Place::Global(_) => continue,
                    };
                    if !interner.is_rc_tracked(f.local_ty(*dest)) {
                        continue;
                    }
                    let sources = rvalue_sources(rv, interner, returns, &locals);
                    if matches!(place, Place::Local(_)) {
                        changed |= locals[dest.0 as usize].extend(&sources);
                    } else {
                        changed |= record_write(*dest, &sources, &mut locals, &mut result);
                    }
                }
            }
            let sources = match &block.terminator {
                Terminator::Return(Some(op)) => operand_sources(op, &locals),
                Terminator::TailCall { callee, args } => apply_call(callee, args, returns, &locals),
                _ => Sources::default(),
            };
            if interner.is_rc_tracked(f.ret) {
                changed |= result.result.extend(&sources);
            }
        }
        if !changed {
            return result;
        }
    }
}

fn operand_sources(op: &Operand, locals: &[Sources]) -> Sources {
    match op {
        Operand::Copy(Place::Local(l))
        | Operand::Copy(Place::Field { base: l, .. })
        | Operand::Copy(Place::Index { base: l, .. })
        | Operand::Copy(Place::Deref { ptr: l, .. }) => locals[l.0 as usize].clone(),
        _ => Sources::default(),
    }
}

fn apply_call(callee: &Callee, args: &[Operand], returns: &Returns, locals: &[Sources]) -> Sources {
    let summary = call_facts(callee, args.len(), returns).result;
    substitute(&summary, args, locals)
}

fn substitute(summary: &Sources, args: &[Operand], locals: &[Sources]) -> Sources {
    let mut sources = Sources {
        fresh: summary.fresh,
        params: BTreeSet::new(),
    };
    for &index in &summary.params {
        if let Some(arg) = args.get(index) {
            sources.extend(&operand_sources(arg, locals));
        }
    }
    sources
}

fn argument_local(op: &Operand) -> Option<crate::Local> {
    match op {
        Operand::Copy(Place::Local(local))
        | Operand::Copy(Place::Field { base: local, .. })
        | Operand::Copy(Place::Index { base: local, .. })
        | Operand::Copy(Place::Deref { ptr: local, .. }) => Some(*local),
        _ => None,
    }
}

fn record_write(
    local: crate::Local,
    sources: &Sources,
    locals: &mut [Sources],
    facts: &mut Facts,
) -> bool {
    let target = &mut locals[local.0 as usize];
    let mut changed = false;
    for &param in &target.params {
        changed |= facts.writes.entry(param).or_default().extend(sources);
    }
    changed | target.extend(sources)
}

fn rvalue_sources(
    rv: &Rvalue,
    interner: &TypeInterner,
    returns: &Returns,
    locals: &[Sources],
) -> Sources {
    if let Rvalue::Call { callee, args } = rv {
        return apply_call(callee, args, returns, locals);
    }
    let mut sources = Sources::default();
    for local in rvalue_local_operands(rv) {
        sources.extend(&locals[local as usize]);
    }
    sources.fresh |= matches!(
        rv,
        Rvalue::New { .. }
            | Rvalue::ArrayLit { .. }
            | Rvalue::ArrayNew { .. }
            | Rvalue::ArrayRealloc { .. }
            | Rvalue::Concat(_)
            | Rvalue::ConcatInt { .. }
            | Rvalue::ToString(_)
            | Rvalue::ToBytes { .. }
            | Rvalue::FromBytes { .. }
            | Rvalue::IndirectCall { .. }
            | Rvalue::InterfaceCall { .. }
            | Rvalue::JsCall { .. }
            | Rvalue::Tuple { .. }
            | Rvalue::StrBytes(_)
    ) || matches!(rv, Rvalue::UnionNew { ty, .. } if !interner.is_niche_union(*ty));
    sources
}
