//! Closed-world AOT: which MIR functions cannot reach a GC safepoint.
//!
//! Direct calls to those functions skip mutator root reloads. Functions that only call other
//! non-safepoint functions (and never allocate) omit the root-table prologue entirely.

use crate::{Callee, Mir, MirFunction, Rvalue, Statement, Terminator};
use dream_types::{DefId, TypeId};
use std::collections::{HashMap, HashSet};

/// `(def, instance)` keys of functions that cannot allocate or call into a collecting callee.
pub(super) fn non_safepoint_functions(mir: &Mir) -> HashSet<(DefId, Vec<TypeId>)> {
    let mut by_key: HashMap<(DefId, Vec<TypeId>), usize> = HashMap::new();
    for (i, f) in mir.functions.iter().enumerate() {
        by_key.insert((f.def, f.instance.clone()), i);
    }
    let mut opaque = vec![false; mir.functions.len()];
    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); mir.functions.len()];
    for (i, f) in mir.functions.iter().enumerate() {
        match direct_callee_keys(f) {
            None => opaque[i] = true,
            Some(keys) => {
                for k in keys {
                    match by_key.get(&k) {
                        Some(&j) if j != i => edges[i].push(j),
                        Some(_) => {}
                        None => opaque[i] = true,
                    }
                }
            }
        }
    }
    let mut non_sp = HashSet::new();
    let mut changed = true;
    while changed {
        changed = false;
        for (i, f) in mir.functions.iter().enumerate() {
            let key = (f.def, f.instance.clone());
            if non_sp.contains(&key) || opaque[i] {
                continue;
            }
            if edges[i].iter().all(|&j| {
                non_sp.contains(&(mir.functions[j].def, mir.functions[j].instance.clone()))
            }) {
                non_sp.insert(key);
                changed = true;
            }
        }
    }
    non_sp
}

/// `None` if this body has a raw safepoint (alloc, JS, indirect/interface, `ValueDrop`, await).
fn direct_callee_keys(func: &MirFunction) -> Option<Vec<(DefId, Vec<TypeId>)>> {
    let mut keys = Vec::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            match stmt {
                Statement::Call { callee, .. } => keys.push(callee_key(callee)),
                Statement::JsCall { .. }
                | Statement::InterfaceCall { .. }
                | Statement::IndirectCall { .. }
                | Statement::ValueDrop(_) => return None,
                Statement::Assign(_, rv) => match rv {
                    Rvalue::Call { callee, .. } => keys.push(callee_key(callee)),
                    other if rvalue_may_safepoint(other) => return None,
                    _ => {}
                },
                _ => {}
            }
        }
        match &block.terminator {
            Terminator::TailCall { callee, .. } => keys.push(callee_key(callee)),
            Terminator::Await { .. } | Terminator::AsyncComplete(_) => return None,
            _ => {}
        }
    }
    Some(keys)
}

fn callee_key(c: &Callee) -> (DefId, Vec<TypeId>) {
    (c.def, c.args.clone())
}

pub(super) fn function_may_safepoint(func: &MirFunction) -> bool {
    for block in &func.blocks {
        for stmt in &block.stmts {
            if stmt_may_safepoint(stmt) {
                return true;
            }
        }
        if term_may_safepoint(&block.terminator) {
            return true;
        }
    }
    false
}

fn stmt_may_safepoint(stmt: &Statement) -> bool {
    match stmt {
        Statement::Assign(_, rv) => rvalue_may_safepoint(rv),
        Statement::Call { .. }
        | Statement::JsCall { .. }
        | Statement::InterfaceCall { .. }
        | Statement::IndirectCall { .. }
        | Statement::ValueDrop(_) => true,
        Statement::Panic(_)
        | Statement::Print { .. }
        | Statement::Nop
        | Statement::DebugLine(_)
        | Statement::SourceLine(_)
        | Statement::ArrayElemsCopy { .. }
        | Statement::ForceFree(_)
        | Statement::LockAcquire(_)
        | Statement::LockRelease(_)
        | Statement::SimdF32x4 { .. } => false,
    }
}

fn term_may_safepoint(term: &Terminator) -> bool {
    matches!(
        term,
        Terminator::TailCall { .. } | Terminator::Await { .. } | Terminator::AsyncComplete(_)
    )
}

pub(super) fn rvalue_may_safepoint(rv: &Rvalue) -> bool {
    matches!(
        rv,
        Rvalue::Call { .. }
            | Rvalue::IndirectCall { .. }
            | Rvalue::InterfaceCall { .. }
            | Rvalue::JsCall { .. }
            | Rvalue::New { .. }
            | Rvalue::UnionNew { .. }
            | Rvalue::ArrayLit { .. }
            | Rvalue::ArrayNew { .. }
            | Rvalue::ArrayRealloc { .. }
            | Rvalue::Concat(_, _)
            | Rvalue::ToString(_)
            | Rvalue::ToBytes { .. }
            | Rvalue::FromBytes { .. }
            | Rvalue::Cast(_, _, _)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use crate::{Const, Operand, Place};
    use dream_types::{DefKind, TypeCtx};

    #[test]
    fn leaf_and_leaf_caller_are_non_safepoint() {
        let mut ctx = TypeCtx::new();
        let int = ctx.interner.int();
        let leaf_def = ctx.register(DefKind::Function, "leaf", vec![]);
        let caller_def = ctx.register(DefKind::Function, "caller", vec![]);

        let mut leaf = FunctionBuilder::new("leaf", int);
        leaf.set_def(leaf_def, vec![]);
        let a = leaf.new_param(int, Some("a".into()));
        let b = leaf.new_param(int, Some("b".into()));
        let t = leaf.new_temp(int);
        leaf.push(Statement::Assign(
            Place::Local(t),
            Rvalue::Binary(
                crate::BinOp::Add,
                Operand::Copy(Place::Local(a)),
                Operand::Copy(Place::Local(b)),
            ),
        ));
        leaf.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));

        let mut caller = FunctionBuilder::new("caller", int);
        caller.set_def(caller_def, vec![]);
        let x = caller.new_param(int, Some("x".into()));
        let y = caller.new_temp(int);
        caller.push(Statement::Assign(
            Place::Local(y),
            Rvalue::Call {
                callee: Callee {
                    def: leaf_def,
                    args: vec![],
                    ret: int,
                },
                args: vec![
                    Operand::Copy(Place::Local(x)),
                    Operand::Const(Const::Int(1)),
                ],
            },
        ));
        caller.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(y)))));

        let mir = Mir {
            functions: vec![leaf.finish(), caller.finish()],
            globals: vec![],
            layouts: Default::default(),
            imports: vec![],
            intrinsics: vec![],
            interfaces: Default::default(),
            enums: Default::default(),
        };
        let set = non_safepoint_functions(&mir);
        assert!(set.contains(&(leaf_def, vec![])));
        assert!(set.contains(&(caller_def, vec![])));
    }
}
