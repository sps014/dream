//! Reference-counting passes.
//!
//! [`RcInsertion`] makes ownership explicit under a single invariant: **every non-parameter
//! reference local owns exactly one reference count** (a compile-time token). [`RcElision`] cancels redundant
//! `Retain`/`Release` pairs when it can prove the cancel is safe (Goto chains, transparent
//! diamonds, transparent natural loops). See `docs/internals/11-swift-like-arc-roadmap.md`.

mod cursor;
mod elision;
mod insertion;
pub(crate) mod liveness;
mod tokens;
mod uniqueness;

#[cfg(test)]
mod cursor_tests;
#[cfg(test)]
mod unique_tests;

pub use elision::RcElision;
pub use insertion::RcInsertion;
pub(crate) use liveness::stmt_reads_local;
pub(crate) use uniqueness::container_move_locals;

use crate::{Global, Local, Operand, Place, Rvalue, Statement};
use dream_types::TypeInterner;

/// A [`Retain`] / [`Release`] operand normalized to the identity it protects, for matching a pair
/// regardless of intervening statements. Only whole-place (`Local`/`Global`) operands are ever the
/// target of a `Retain`/`Release` — [`RcKey::of`] returns `None` for anything else.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum RcKey {
    Local(Local),
    Global(Global),
}

impl RcKey {
    pub(crate) fn of(op: &Operand) -> Option<RcKey> {
        match op {
            Operand::Copy(Place::Local(l)) => Some(RcKey::Local(*l)),
            Operand::Copy(Place::Global(g)) => Some(RcKey::Global(*g)),
            _ => None,
        }
    }
}

/// True for an [`Rvalue`] that reads only its operands with no possible side effect: no allocation,
/// no call, no runtime helper that could itself retain/release/inspect an object's refcount. Exactly
/// the rvalues elision permits between a pending `Retain` and its `Release` without treating them as
/// a barrier.
pub(crate) fn is_pure_rvalue(rvalue: &Rvalue) -> bool {
    matches!(
        rvalue,
        Rvalue::Use(_) | Rvalue::Select { .. } | Rvalue::Binary(..) | Rvalue::Unary(..)
    )
}

/// True if `stmt` cannot observe or change any object's refcount (and is not itself a Retain/Release).
pub(crate) fn is_transparent_stmt(stmt: &Statement) -> bool {
    match stmt {
        Statement::Assign(Place::Local(_), rvalue) if is_pure_rvalue(rvalue) => true,
        Statement::Print { .. }
        | Statement::DebugLine(_)
        | Statement::SourceLine(_)
        | Statement::Nop => true,
        _ => false,
    }
}

/// True if `local` is read anywhere in `rvalue` (as a plain operand or through a field/index base).
pub(crate) fn rvalue_reads_local(rvalue: &Rvalue, local: u32) -> bool {
    let mut hit = false;
    let mut check = |op: &Operand| {
        if let Operand::Copy(place) = op {
            let base = match place {
                Place::Local(l) => Some(l.0),
                Place::Field { base, .. } => Some(base.0),
                Place::Index { base, .. } => Some(base.0),
                Place::Deref { ptr, .. } => Some(ptr.0),
                Place::Global(_) => None,
            };
            if base == Some(local) {
                hit = true;
            }
            if let Place::Index { index, .. } = place {
                if let Operand::Copy(Place::Local(l)) = index.as_ref() {
                    if l.0 == local {
                        hit = true;
                    }
                }
            }
        }
    };
    match rvalue {
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => {
            check(cond);
            check(then_val);
            check(else_val);
        }
        Rvalue::Use(o)
        | Rvalue::Unary(_, o)
        | Rvalue::ArrayLen(o)
        | Rvalue::StrLen(o)
        | Rvalue::StrByteSize(o)
        | Rvalue::Cast(o, _, _)
        | Rvalue::IsType(o, _)
        | Rvalue::Discriminant(o)
        | Rvalue::HashCode(o)
        | Rvalue::ToString(o)
        | Rvalue::UnionField { base: o, .. } => check(o),
        Rvalue::Binary(_, a, b) | Rvalue::CharAt(a, b, _) | Rvalue::ByteAt(a, b, _) => {
            check(a);
            check(b);
        }
        Rvalue::Concat(parts) => {
            for p in parts {
                check(p);
            }
        }
        Rvalue::ConcatInt {
            prefix,
            value,
            suffix,
        } => {
            check(prefix);
            check(value);
            check(suffix);
        }
        Rvalue::EnumName { value, .. } => check(value),
        Rvalue::ArrayNew { len, .. } => check(len),
        Rvalue::ToBytes { value: o, .. } | Rvalue::FromBytes { bytes: o, .. } => check(o),
        Rvalue::ArrayRealloc { array, new_len, .. } => {
            check(array);
            check(new_len);
        }
        Rvalue::Call { args, .. }
        | Rvalue::New { args, .. }
        | Rvalue::UnionNew { args, .. }
        | Rvalue::ArrayLit { elems: args, .. }
        | Rvalue::Tuple { elems: args, .. } => args.iter().for_each(&mut check),
        Rvalue::IndirectCall { target, args, .. } => {
            check(target);
            args.iter().for_each(&mut check);
        }
        Rvalue::InterfaceCall { receiver, args, .. } => {
            check(receiver);
            args.iter().for_each(&mut check);
        }
        Rvalue::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            check(target);
            if let Some(v) = via {
                check(v);
            }
            if let Some(m) = method {
                check(m);
            }
            args.iter().for_each(|(a, _)| check(a));
        }
        Rvalue::FuncRef(_) => {}
    }
    hit
}

/// True if the rvalue is a *borrow* that must be retained when bound to an owning local.
pub(crate) fn is_borrowed_copy(rvalue: &Rvalue, interner: &TypeInterner) -> bool {
    match rvalue {
        Rvalue::Use(Operand::Copy(_))
        | Rvalue::Use(Operand::Const(crate::Const::Str(_)))
        | Rvalue::UnionField { .. } => true,
        Rvalue::Cast(Operand::Copy(_), from, to) => {
            if !interner.is_rc_tracked(*to) {
                return false;
            }
            interner.is_rc_tracked(*from)
                || matches!(
                    interner.kind(*from),
                    dream_types::TyKind::Prim(dream_types::PrimTy::Int)
                )
        }
        _ => false,
    }
}
