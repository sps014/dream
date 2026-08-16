//! Intra-block copy and constant propagation. Within a single basic block, a local that is assigned
//! a constant or a copy of another local has its later reads replaced by the source value. The
//! analysis is reset at block boundaries (no cross-block dataflow), which keeps it simple and sound
//! without SSA phi handling.
//!
//! Value-struct locals are excluded: their WASM locals hold *addresses* of shadow-frame slots, and an
//! Owning `Assign` deep-copies into a distinct slot. Propagating the address would make two MIR
//! locals share one slot, so a later `ValueDrop` / frame teardown of the source would free the
//! destination's storage too.

use super::MirPass;
use crate::{Local, MirFunction, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::TypeInterner;
use std::collections::HashMap;

pub struct CopyConstProp;

impl MirPass for CopyConstProp {
    fn name(&self) -> &'static str {
        "copy-const-prop"
    }

    fn run(&self, func: &mut MirFunction, interner: &TypeInterner) -> bool {
        let value_local: Vec<bool> = func
            .locals
            .iter()
            .map(|d| interner.is_value_type(d.ty))
            .collect();
        let mut changed = false;
        for block in &mut func.blocks {
            let mut known: HashMap<Local, Operand> = HashMap::new();
            for stmt in &mut block.stmts {
                changed |= subst_stmt_reads(stmt, &known);
                update_known(stmt, &mut known, &value_local);
            }
            changed |= subst_terminator_reads(&mut block.terminator, &known);
        }
        changed
    }
}

/// Resolves an operand through the known-value map (chasing copies transitively).
fn resolve(op: &Operand, known: &HashMap<Local, Operand>) -> Option<Operand> {
    if let Operand::Copy(Place::Local(l)) = op {
        if let Some(v) = known.get(l) {
            // Chase further in case `v` is itself a propagated copy.
            return Some(resolve(v, known).unwrap_or_else(|| v.clone()));
        }
    }
    None
}

fn subst_operand(op: &mut Operand, known: &HashMap<Local, Operand>) -> bool {
    if let Some(v) = resolve(op, known) {
        *op = v;
        return true;
    }
    false
}

fn subst_place_reads(place: &mut Place, known: &HashMap<Local, Operand>) -> bool {
    // Only the index operand of an `Index` place is a *read*; the base local is a destination/base.
    if let Place::Index { index, .. } = place {
        return subst_operand(index, known);
    }
    false
}

pub(super) fn subst_stmt_reads(stmt: &mut Statement, known: &HashMap<Local, Operand>) -> bool {
    match stmt {
        Statement::Assign(place, rvalue) => {
            let mut c = subst_place_reads(place, known);
            c |= subst_rvalue_reads(rvalue, known);
            c
        }
        Statement::Retain(o) | Statement::Release(o) | Statement::Panic(o) => {
            subst_operand(o, known)
        }
        Statement::Call { args, .. } => args
            .iter_mut()
            .fold(false, |c, a| c | subst_operand(a, known)),
        Statement::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            let mut c = subst_operand(target, known);
            if let Some(v) = via {
                c |= subst_operand(v, known);
            }
            if let Some(m) = method {
                c |= subst_operand(m, known);
            }
            for (a, _) in args {
                c |= subst_operand(a, known);
            }
            c
        }
        Statement::InterfaceCall { receiver, args, .. } => {
            let mut c = subst_operand(receiver, known);
            for a in args {
                c |= subst_operand(a, known);
            }
            c
        }
        Statement::IndirectCall { target, args, .. } => {
            let mut c = subst_operand(target, known);
            for a in args {
                c |= subst_operand(a, known);
            }
            c
        }
        Statement::Print { arg, .. } => subst_operand(arg, known),
        Statement::ForceFree(o) => subst_operand(o, known),
        Statement::ValueDrop(_) | Statement::ValueRetain(_) | Statement::ValueKill(_) => false,
        Statement::ArrayElemsCopy {
            dst,
            dst_off,
            src,
            src_off,
            count,
            ..
        } => {
            subst_operand(dst, known)
                | subst_operand(dst_off, known)
                | subst_operand(src, known)
                | subst_operand(src_off, known)
                | subst_operand(count, known)
        }
        Statement::LockAcquire(o) | Statement::LockRelease(o) => subst_operand(o, known),
        Statement::SimdF32x4 {
            dest,
            lhs,
            rhs,
            index,
            ..
        } => {
            subst_operand(dest, known)
                | subst_operand(lhs, known)
                | subst_operand(rhs, known)
                | subst_operand(index, known)
        }
        Statement::Nop | Statement::DebugLine(_) | Statement::SourceLine(_) => false,
    }
}

fn subst_rvalue_reads(rvalue: &mut Rvalue, known: &HashMap<Local, Operand>) -> bool {
    match rvalue {
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => {
            subst_operand(cond, known)
                | subst_operand(then_val, known)
                | subst_operand(else_val, known)
        }
        Rvalue::Use(o)
        | Rvalue::ArrayLen(o)
        | Rvalue::StrLen(o)
        | Rvalue::StrByteSize(o)
        | Rvalue::Cast(o, _, _)
        | Rvalue::IsType(o, _)
        | Rvalue::Discriminant(o)
        | Rvalue::HashCode(o)
        | Rvalue::ToString(o)
        | Rvalue::UnionField { base: o, .. } => subst_operand(o, known),
        Rvalue::Binary(_, a, b)
        | Rvalue::CharAt(a, b)
        | Rvalue::ByteAt(a, b)
        | Rvalue::Concat(a, b) => subst_operand(a, known) | subst_operand(b, known),
        Rvalue::EnumName { value, .. } => subst_operand(value, known),
        Rvalue::ArrayNew { len, .. } => subst_operand(len, known),
        Rvalue::ToBytes { value: o, .. } | Rvalue::FromBytes { bytes: o, .. } => {
            subst_operand(o, known)
        }
        Rvalue::ArrayRealloc { array, new_len, .. } => {
            subst_operand(array, known) | subst_operand(new_len, known)
        }
        Rvalue::Unary(_, a) => subst_operand(a, known),
        Rvalue::Call { args, .. }
        | Rvalue::New { args, .. }
        | Rvalue::UnionNew { args, .. }
        | Rvalue::ArrayLit { elems: args, .. }
        | Rvalue::Tuple { elems: args, .. } => args
            .iter_mut()
            .fold(false, |c, a| c | subst_operand(a, known)),
        Rvalue::IndirectCall { target, args, .. } => {
            let mut c = subst_operand(target, known);
            for a in args {
                c |= subst_operand(a, known);
            }
            c
        }
        Rvalue::InterfaceCall { receiver, args, .. } => {
            let mut c = subst_operand(receiver, known);
            for a in args {
                c |= subst_operand(a, known);
            }
            c
        }
        Rvalue::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            let mut c = subst_operand(target, known);
            if let Some(v) = via {
                c |= subst_operand(v, known);
            }
            if let Some(m) = method {
                c |= subst_operand(m, known);
            }
            for (a, _) in args {
                c |= subst_operand(a, known);
            }
            c
        }
        Rvalue::FuncRef(_) => false,
    }
}

pub(super) fn subst_terminator_reads(t: &mut Terminator, known: &HashMap<Local, Operand>) -> bool {
    match t {
        Terminator::If { cond, .. } => subst_operand(cond, known),
        Terminator::Switch { value, .. } => subst_operand(value, known),
        Terminator::Return(Some(o)) => subst_operand(o, known),
        Terminator::AsyncComplete(Some(o)) => subst_operand(o, known),
        Terminator::TailCall { args, .. } => args
            .iter_mut()
            .fold(false, |c, a| c | subst_operand(a, known)),
        _ => false,
    }
}

/// Updates the known-value map after a statement executes.
pub(super) fn update_known(
    stmt: &Statement,
    known: &mut HashMap<Local, Operand>,
    value_local: &[bool],
) {
    let is_value = |l: Local| value_local.get(l.0 as usize).copied().unwrap_or(false);
    if let Statement::Assign(Place::Local(dest), rvalue) = stmt {
        // The destination's old value is gone, and any entry that *copied* it is now stale.
        invalidate(*dest, known);
        if let Rvalue::Use(op @ (Operand::Const(_) | Operand::Copy(Place::Local(_)))) = rvalue {
            let value_typed =
                is_value(*dest) || matches!(op, Operand::Copy(Place::Local(src)) if is_value(*src));
            if !value_typed {
                known.insert(*dest, op.clone());
            }
        }
    } else if let Statement::Assign(_, _) = stmt {
        // Stores through field/index/global may alias; be conservative and keep only consts.
        known.retain(|_, v| matches!(v, Operand::Const(_)));
    } else if let Statement::ValueDrop(l)
        | Statement::ValueRetain(l)
        | Statement::ValueKill(l) = stmt
    {
        invalidate(*l, known);
    }
    // Calls may mutate through references; constants stay valid, copies of locals are kept (locals
    // are not aliased by value here).
}

fn invalidate(dest: Local, known: &mut HashMap<Local, Operand>) {
    known.remove(&dest);
    known.retain(|_, v| !matches!(v, Operand::Copy(Place::Local(l)) if *l == dest));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use crate::{Const, Operand, Place, Rvalue, Terminator};

    #[test]
    fn propagates_const_into_return() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.int());
        let x = b.new_temp(i.int());
        b.assign(Place::Local(x), Rvalue::Use(Operand::Const(Const::Int(7))));
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(x)))));
        let mut func = b.finish();
        assert!(CopyConstProp.run(&mut func, &i));
        match &func.blocks[0].terminator {
            Terminator::Return(Some(Operand::Const(Const::Int(v)))) => assert_eq!(*v, 7),
            other => panic!("expected propagated const, got {:?}", other),
        }
    }
}
