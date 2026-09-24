//! Mutable walks over every operand a statement, rvalue or terminator reads (including an
//! `Index` place's index, in destinations as well as operands).

use crate::{Operand, Place, Rvalue, Statement, Terminator};

pub(crate) fn stmt_operands_mut(s: &mut Statement, f: &mut impl FnMut(&mut Operand)) {
    match s {
        Statement::Assign(place, rv) => {
            place_operands_mut(place, f);
            rvalue_operands_mut(rv, f);
        }
        Statement::Retain(o)
        | Statement::Release(o)
        | Statement::ReleaseUnique(o)
        | Statement::Panic(o)
        | Statement::ForceFree(o)
        | Statement::LockAcquire(o)
        | Statement::LockRelease(o)
        | Statement::DeferLeave(o)
        | Statement::Print { arg: o, .. } => operand_mut(o, f),
        Statement::Call { args, .. } => args.iter_mut().for_each(|a| operand_mut(a, f)),
        Statement::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            operand_mut(target, f);
            via.iter_mut().chain(method).for_each(|o| operand_mut(o, f));
            args.iter_mut().for_each(|(a, _)| operand_mut(a, f));
        }
        Statement::InterfaceCall { receiver, args, .. } => {
            operand_mut(receiver, f);
            args.iter_mut().for_each(|a| operand_mut(a, f));
        }
        Statement::IndirectCall { target, args, .. } => {
            operand_mut(target, f);
            args.iter_mut().for_each(|a| operand_mut(a, f));
        }
        Statement::ArrayElemsCopy {
            dst,
            dst_off,
            src,
            src_off,
            count,
            ..
        } => {
            for o in [dst, dst_off, src, src_off, count] {
                operand_mut(o, f);
            }
        },
        Statement::ArrayElemsFill {
            dst,
            dst_off,
            count,
            ..
        } => {
            for o in [dst, dst_off, count] {
                operand_mut(o, f);
            }
        },
        Statement::SimdV128 {
            dest,
            lhs,
            rhs,
            index,
            splat_rhs,
            ..
        } => {
            {
            for o in [dest, lhs, rhs, index] {
                operand_mut(o, f);
            }
        };
            splat_rhs.iter_mut().for_each(|o| operand_mut(o, f));
        }
        Statement::Nop
        | Statement::DebugLine(_)
        | Statement::SourceLine(_)
        | Statement::DeferEnter
        | Statement::RegionEnter
        | Statement::RegionLeave
        | Statement::ValueDrop(_)
        | Statement::ValueRetain(_)
        | Statement::ValueKill(_) => {}
    }
}

pub(crate) fn rvalue_operands_mut(rv: &mut Rvalue, f: &mut impl FnMut(&mut Operand)) {
    match rv {
        Rvalue::Move { .. } | Rvalue::FuncRef(_) => {}
        Rvalue::Use(o)
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
        | Rvalue::FromBytes { bytes: o, .. }
        | Rvalue::Unary(_, o)
        | Rvalue::CheckedNeg(o) => operand_mut(o, f),
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
        } => {
            operand_mut(a, f);
            operand_mut(b, f);
        }
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => {
            for o in [cond, then_val, else_val] {
                operand_mut(o, f);
            }
        },
        Rvalue::ConcatInt {
            prefix,
            value,
            suffix,
        } => {
            for o in [prefix, value, suffix] {
                operand_mut(o, f);
            }
        },
        Rvalue::Concat(ops)
        | Rvalue::Call { args: ops, .. }
        | Rvalue::New { args: ops, .. }
        | Rvalue::UnionNew { args: ops, .. }
        | Rvalue::ArrayLit { elems: ops, .. }
        | Rvalue::Tuple { elems: ops, .. } => ops.iter_mut().for_each(|o| operand_mut(o, f)),
        Rvalue::IndirectCall { target, args, .. } => {
            operand_mut(target, f);
            args.iter_mut().for_each(|a| operand_mut(a, f));
        }
        Rvalue::InterfaceCall { receiver, args, .. } => {
            operand_mut(receiver, f);
            args.iter_mut().for_each(|a| operand_mut(a, f));
        }
        Rvalue::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            operand_mut(target, f);
            via.iter_mut().chain(method).for_each(|o| operand_mut(o, f));
            args.iter_mut().for_each(|(a, _)| operand_mut(a, f));
        }
    }
}

pub(crate) fn terminator_operands_mut(t: &mut Terminator, f: &mut impl FnMut(&mut Operand)) {
    match t {
        Terminator::If { cond: o, .. }
        | Terminator::Switch { value: o, .. }
        | Terminator::Return(Some(o))
        | Terminator::AsyncComplete(Some(o))
        | Terminator::Await { future: o, .. } => operand_mut(o, f),
        Terminator::TailCall { args, .. } => args.iter_mut().for_each(|a| operand_mut(a, f)),
        Terminator::Goto(_)
        | Terminator::Return(None)
        | Terminator::AsyncComplete(None)
        | Terminator::Unreachable => {}
    }
}

fn place_operands_mut(p: &mut Place, f: &mut impl FnMut(&mut Operand)) {
    if let Place::Index { index, .. } = p {
        operand_mut(index, f);
    }
}

fn operand_mut(o: &mut Operand, f: &mut impl FnMut(&mut Operand)) {
    f(o);
    if let Operand::Copy(Place::Index { index, .. }) = o {
        operand_mut(index, f);
    }
}
