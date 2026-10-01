//! Exhaustive operand readers shared by verifier analyses.

use crate::{Operand, Place, Rvalue, Statement, Terminator};
use std::collections::BTreeSet;

pub(super) fn operand_locals(o: &Operand, out: &mut Vec<u32>) {
    if let Operand::Copy(p) = o {
        match p {
            Place::Local(l) => out.push(l.0),
            Place::Field { base, .. } | Place::Deref { ptr: base, .. } => out.push(base.0),
            Place::Index { base, index, .. } => {
                out.push(base.0);
                operand_locals(index, out);
            }
            Place::Global(_) => {}
        }
    }
}

pub(super) fn rvalue_local_operands(rv: &Rvalue) -> Vec<u32> {
    let mut out = Vec::new();
    let mut ops: Vec<&Operand> = Vec::new();
    match rv {
        Rvalue::Move { src, .. } => out.push(src.0),
        Rvalue::Use(o)
        | Rvalue::Unary(_, o)
        | Rvalue::CheckedNeg(o)
        | Rvalue::ArrayLen(o)
        | Rvalue::StrLen(o)
        | Rvalue::StrByteSize(o)
        | Rvalue::StrBytes(o)
        | Rvalue::Cast(o, _, _)
        | Rvalue::IsType(o, _)
        | Rvalue::TypeName(o)
        | Rvalue::Discriminant { base: o, .. }
        | Rvalue::HashCode(o)
        | Rvalue::ToString(o)
        | Rvalue::UnionField { base: o, .. }
        | Rvalue::EnumName { value: o, .. }
        | Rvalue::ArrayNew { len: o, .. }
        | Rvalue::ToBytes { value: o, .. }
        | Rvalue::FromBytes { bytes: o, .. } => ops.push(o),
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => ops.extend([cond, then_val, else_val]),
        Rvalue::Binary(_, a, b)
        | Rvalue::CheckedBinary(_, a, b)
        | Rvalue::CharAt(a, b, _)
        | Rvalue::ByteAt(a, b, _)
        | Rvalue::LoadU8(a, b)
        | Rvalue::LoadU16(a, b)
        | Rvalue::ArrayRealloc {
            array: a,
            new_len: b,
            ..
        } => ops.extend([a, b]),
        Rvalue::ConcatInt {
            prefix,
            value,
            suffix,
        } => ops.extend([prefix, value, suffix]),
        Rvalue::Concat(args)
        | Rvalue::Call { args, .. }
        | Rvalue::New { args, .. }
        | Rvalue::UnionNew { args, .. }
        | Rvalue::ArrayLit { elems: args, .. }
        | Rvalue::Tuple { elems: args, .. } => ops.extend(args.iter()),
        Rvalue::IndirectCall { target, args, .. } => {
            ops.push(target);
            ops.extend(args.iter());
        }
        Rvalue::InterfaceCall { receiver, args, .. } => {
            ops.push(receiver);
            ops.extend(args.iter());
        }
        Rvalue::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            ops.push(target);
            ops.extend(via.iter());
            ops.extend(method.iter());
            ops.extend(args.iter().map(|(a, _)| a));
        }
        Rvalue::FuncRef(_) => {}
    }
    for o in ops {
        operand_locals(o, &mut out);
    }
    out
}

pub(super) fn terminator_reads(t: &Terminator) -> BTreeSet<u32> {
    let mut out = Vec::new();
    match t {
        Terminator::If { cond: o, .. }
        | Terminator::Switch { value: o, .. }
        | Terminator::Return(Some(o))
        | Terminator::AsyncComplete(Some(o))
        | Terminator::Await { future: o, .. } => operand_locals(o, &mut out),
        Terminator::TailCall { args, .. } => args.iter().for_each(|a| operand_locals(a, &mut out)),
        _ => {}
    }
    out.into_iter().collect()
}

/// Locals read by a statement that is not an `Assign`/RC op/marker — every one of these
/// (calls, prints, locks, SIMD, value glue, …) may retain, store, or inspect its operands.
pub(super) fn other_stmt_locals(s: &Statement) -> Vec<u32> {
    let mut out = Vec::new();
    let mut ops: Vec<&Operand> = Vec::new();
    match s {
        Statement::Panic(o)
        | Statement::ForceFree(o)
        | Statement::LockAcquire(o)
        | Statement::LockRelease(o)
        | Statement::DeferLeave(o)
        | Statement::Print { arg: o, .. } => ops.push(o),
        Statement::Call { args, .. } => ops.extend(args.iter()),
        Statement::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            ops.push(target);
            ops.extend(via.iter());
            ops.extend(method.iter());
            ops.extend(args.iter().map(|(a, _)| a));
        }
        Statement::InterfaceCall { receiver, args, .. } => {
            ops.push(receiver);
            ops.extend(args.iter());
        }
        Statement::IndirectCall { target, args, .. } => {
            ops.push(target);
            ops.extend(args.iter());
        }
        Statement::ArrayElemsCopy {
            dst,
            dst_off,
            src,
            src_off,
            count,
            ..
        } => ops.extend([dst, dst_off, src, src_off, count]),
        Statement::ArrayElemsFill {
            dst,
            dst_off,
            count,
            ..
        } => ops.extend([dst, dst_off, count]),
        Statement::SimdV128 {
            dest,
            lhs,
            rhs,
            index,
            splat_rhs,
            ..
        } => {
            ops.extend([dest, lhs, rhs, index]);
            ops.extend(splat_rhs.iter());
        }
        Statement::ValueDrop(l) | Statement::ValueRetain(l) | Statement::ValueKill(l) => {
            out.push(l.0)
        }
        Statement::Assign(..)
        | Statement::Retain(_)
        | Statement::Release(_)
        | Statement::Nop
        | Statement::DebugLine(_)
        | Statement::SourceLine(_)
        | Statement::DeferEnter
        | Statement::RegionEnter
        | Statement::RegionLeave => {}
    }
    for o in ops {
        operand_locals(o, &mut out);
    }
    out
}
