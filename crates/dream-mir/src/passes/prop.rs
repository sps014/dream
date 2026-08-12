//! Intra-block copy and constant propagation. Within a single basic block, a local that is assigned
//! a constant or a copy of another local has its later reads replaced by the source value. The
//! analysis is reset at block boundaries (no cross-block dataflow), which keeps it simple and sound
//! without SSA phi handling.
//!
//! Value-struct locals are excluded: their WASM locals hold *addresses* of shadow-frame slots, and an
//! Owning `Assign` deep-copies into a distinct slot. Propagating the address would make two MIR
//! locals share one slot, so a later `ValueDrop` / frame teardown of the source would free the
//! destination's storage too.
//!
//! Take (`is_take`) locals are also excluded from substitution. Emit `take_transfer` nulls the store
//! operand and skips an extra retain; a later `Release` on that same local must still free the
//! object. Propagating a take local to its source (e.g. after inlining `next = v`) would make the
//! field store null `v` while `Release(next)` still drops — double-free / freelist UAF.

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
        let is_take: Vec<bool> = func.locals.iter().map(|d| d.is_take).collect();
        let mut changed = false;
        for block in &mut func.blocks {
            let mut known: HashMap<Local, Operand> = HashMap::new();
            for stmt in &mut block.stmts {
                changed |= subst_stmt_reads(stmt, &known, &is_take);
                update_known(stmt, &mut known, &value_local);
            }
            changed |= subst_terminator_reads(&mut block.terminator, &known, &is_take);
        }
        changed
    }
}

/// Resolves an operand through the known-value map (chasing copies transitively).
fn resolve(op: &Operand, known: &HashMap<Local, Operand>, is_take: &[bool]) -> Option<Operand> {
    if let Operand::Copy(Place::Local(l)) = op {
        // Keep uses of take locals intact so emit take_transfer and Release share one identity.
        if is_take.get(l.0 as usize).copied().unwrap_or(false) {
            return None;
        }
        if let Some(v) = known.get(l) {
            // Chase further in case `v` is itself a propagated copy.
            return Some(resolve(v, known, is_take).unwrap_or_else(|| v.clone()));
        }
    }
    None
}

fn subst_operand(op: &mut Operand, known: &HashMap<Local, Operand>, is_take: &[bool]) -> bool {
    if let Some(v) = resolve(op, known, is_take) {
        *op = v;
        return true;
    }
    false
}

fn subst_place_reads(
    place: &mut Place,
    known: &HashMap<Local, Operand>,
    is_take: &[bool],
) -> bool {
    // Only the index operand of an `Index` place is a *read*; the base local is a destination/base.
    if let Place::Index { index, .. } = place {
        return subst_operand(index, known, is_take);
    }
    false
}

pub(super) fn subst_stmt_reads(
    stmt: &mut Statement,
    known: &HashMap<Local, Operand>,
    is_take: &[bool],
) -> bool {
    match stmt {
        Statement::Assign(place, rvalue) => {
            let mut c = subst_place_reads(place, known, is_take);
            c |= subst_rvalue_reads(rvalue, known, is_take);
            c
        }
        Statement::Panic(o) => {
            subst_operand(o, known, is_take)
        }
        Statement::Call { args, .. } => args
            .iter_mut()
            .fold(false, |c, a| c | subst_operand(a, known, is_take)),
        Statement::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            let mut c = subst_operand(target, known, is_take);
            if let Some(v) = via {
                c |= subst_operand(v, known, is_take);
            }
            if let Some(m) = method {
                c |= subst_operand(m, known, is_take);
            }
            for (a, _) in args {
                c |= subst_operand(a, known, is_take);
            }
            c
        }
        Statement::InterfaceCall { receiver, args, .. } => {
            let mut c = subst_operand(receiver, known, is_take);
            for a in args {
                c |= subst_operand(a, known, is_take);
            }
            c
        }
        Statement::IndirectCall { target, args, .. } => {
            let mut c = subst_operand(target, known, is_take);
            for a in args {
                c |= subst_operand(a, known, is_take);
            }
            c
        }
        Statement::Print { arg, .. } => subst_operand(arg, known, is_take),
        Statement::ForceFree(o) => subst_operand(o, known, is_take),
        Statement::ValueDrop(_) => false,
        Statement::ArrayElemsCopy {
            dst,
            dst_off,
            src,
            src_off,
            count,
            ..
        } => {
            subst_operand(dst, known, is_take)
                | subst_operand(dst_off, known, is_take)
                | subst_operand(src, known, is_take)
                | subst_operand(src_off, known, is_take)
                | subst_operand(count, known, is_take)
        }
        Statement::LockAcquire(o) | Statement::LockRelease(o) => subst_operand(o, known, is_take),
        Statement::Nop | Statement::DebugLine(_) | Statement::SourceLine(_) => false,
    }
}

fn subst_rvalue_reads(
    rvalue: &mut Rvalue,
    known: &HashMap<Local, Operand>,
    is_take: &[bool],
) -> bool {
    match rvalue {
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => {
            subst_operand(cond, known, is_take)
                | subst_operand(then_val, known, is_take)
                | subst_operand(else_val, known, is_take)
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
        | Rvalue::UnionField { base: o, .. } => subst_operand(o, known, is_take),
        Rvalue::Binary(_, a, b) | Rvalue::CharAt(a, b) | Rvalue::ByteAt(a, b) | Rvalue::Concat(a, b) => {
            subst_operand(a, known, is_take) | subst_operand(b, known, is_take)
        }
        Rvalue::EnumName { value, .. } => subst_operand(value, known, is_take),
        Rvalue::ArrayNew { len, .. } => subst_operand(len, known, is_take),
        Rvalue::ToBytes { value: o, .. } | Rvalue::FromBytes { bytes: o, .. } => {
            subst_operand(o, known, is_take)
        }
        Rvalue::ArrayRealloc { array, new_len, .. } => {
            subst_operand(array, known, is_take) | subst_operand(new_len, known, is_take)
        }
        Rvalue::Unary(_, a) => subst_operand(a, known, is_take),
        Rvalue::Call { args, .. }
        | Rvalue::New { args, .. }
        | Rvalue::UnionNew { args, .. }
        | Rvalue::ArrayLit { elems: args, .. }
        | Rvalue::Tuple { elems: args, .. } => args
            .iter_mut()
            .fold(false, |c, a| c | subst_operand(a, known, is_take)),
        Rvalue::IndirectCall { target, args, .. } => {
            let mut c = subst_operand(target, known, is_take);
            for a in args {
                c |= subst_operand(a, known, is_take);
            }
            c
        }
        Rvalue::InterfaceCall { receiver, args, .. } => {
            let mut c = subst_operand(receiver, known, is_take);
            for a in args {
                c |= subst_operand(a, known, is_take);
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
            let mut c = subst_operand(target, known, is_take);
            if let Some(v) = via {
                c |= subst_operand(v, known, is_take);
            }
            if let Some(m) = method {
                c |= subst_operand(m, known, is_take);
            }
            for (a, _) in args {
                c |= subst_operand(a, known, is_take);
            }
            c
        }
        Rvalue::FuncRef(_) => false,
    }
}

pub(super) fn subst_terminator_reads(
    t: &mut Terminator,
    known: &HashMap<Local, Operand>,
    is_take: &[bool],
) -> bool {
    match t {
        Terminator::If { cond, .. } => subst_operand(cond, known, is_take),
        Terminator::Switch { value, .. } => subst_operand(value, known, is_take),
        Terminator::Return(Some(o)) => subst_operand(o, known, is_take),
        Terminator::AsyncComplete(Some(o)) => subst_operand(o, known, is_take),
        Terminator::TailCall { args, .. } => args
            .iter_mut()
            .fold(false, |c, a| c | subst_operand(a, known, is_take)),
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
            let value_typed = is_value(*dest)
                || matches!(op, Operand::Copy(Place::Local(src)) if is_value(*src));
            if !value_typed {
                known.insert(*dest, op.clone());
            }
        }
    } else if let Statement::Assign(_, _) = stmt {
        // Stores through field/index/global may alias; be conservative and keep only consts.
        known.retain(|_, v| matches!(v, Operand::Const(_)));
    } else if let Statement::ValueDrop(l) = stmt {
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
    use crate::{Const, Operand, Place, Rvalue, Statement, Terminator};

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

    /// After inlining a take-param setter, `next = src` must not rewrite the field store away from
    /// `next` when `next` is marked `is_take`.
    #[test]
    fn does_not_propagate_take_local_into_field_store() {
        let i = TypeInterner::new();
        let obj = i.object();
        let mut b = FunctionBuilder::new("f", i.void());
        let this = b.new_param(obj, Some("this".into()));
        let src = b.new_param(obj, Some("v".into()));
        let next = b.new_temp(obj);
        b.assign(
            Place::Local(next),
            Rvalue::Use(Operand::Copy(Place::Local(src))),
        );
        b.assign(
            Place::Field {
                base: this,
                field: 0,
            },
            Rvalue::Use(Operand::Copy(Place::Local(next))),
        );
        b.terminate(Terminator::Return(None));
        let mut func = b.finish();
        func.locals[next.0 as usize].is_take = true;
        let _ = CopyConstProp.run(&mut func, &i);
        let stmts = &func.blocks[0].stmts;
        let store = stmts
            .iter()
            .find(|s| matches!(s, Statement::Assign(Place::Field { .. }, _)))
            .expect("field store");
        match store {
            Statement::Assign(
                Place::Field { .. },
                Rvalue::Use(Operand::Copy(Place::Local(l))),
            ) => assert_eq!(*l, next, "field store must keep the take local"),
            other => panic!("unexpected field store: {:?}", other),
        }
    }
}
