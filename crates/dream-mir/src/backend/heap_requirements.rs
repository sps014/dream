//! Conservative proof for programs whose generated entry cannot perform guest heap work.

use crate::{Const, Mir, Operand, Place, Rvalue, Statement};
use dream_types::{PrimTy, TyKind, TypeId, TypeInterner};
use std::collections::BTreeSet;

pub(super) fn required(mir: &Mir, interner: &TypeInterner, embedding: bool) -> bool {
    if embedding
        || mir.uses_defer
        || !mir.polls.is_empty()
        || !mir.intrinsics.is_empty()
        || mir
            .imports
            .iter()
            .any(|i| !dream_abi::runtime_hosts::is_pure_math_env_import(&i.module, &i.field))
        || mir
            .globals
            .iter()
            .any(|g| g.id.0 != 0 && !scalar(interner, g.ty))
    {
        return true;
    }
    let bodies: BTreeSet<_> = mir
        .functions
        .iter()
        .map(|f| (f.def, f.instance.as_slice()))
        .collect();
    mir.functions.iter().any(|f| {
        f.is_async
            || (f.name == crate::abi::ENTRY_FN && !f.params.is_empty())
            || !scalar(interner, f.ret)
            || f.locals.iter().any(|l| !scalar(interner, l.ty))
            || f.blocks.iter().any(|b| {
                b.stmts.iter().any(|s| match s {
                    Statement::Nop | Statement::SourceLine(_) | Statement::DebugLine(_) => false,
                    Statement::Assign(Place::Local(_) | Place::Global(_), rv) => match rv {
                        Rvalue::Use(_)
                        | Rvalue::Zero(_)
                        | Rvalue::Unary(..)
                        | Rvalue::Binary(..)
                        | Rvalue::CheckedBinary(..)
                        | Rvalue::CheckedNeg(_)
                        | Rvalue::Select { .. }
                        | Rvalue::Move { .. }
                        | Rvalue::Cast(..) => false,
                        Rvalue::Call { callee, .. } => {
                            !bodies.contains(&(callee.def, callee.args.as_slice()))
                        }
                        _ => true,
                    },
                    Statement::Call { callee, .. } => {
                        !bodies.contains(&(callee.def, callee.args.as_slice()))
                    }
                    Statement::Print { arg, ty, .. } => {
                        !matches!(
                            interner.kind(*ty),
                            TyKind::Prim(
                                PrimTy::Int | PrimTy::Char | PrimTy::Float | PrimTy::Double
                            )
                        ) && !matches!(arg, Operand::Const(Const::Str(_)))
                    }
                    // Panic hooks are opaque; their implementation can allocate even when the
                    // failing path itself contains only scalar work.
                    _ => true,
                })
            })
    })
}

fn scalar(interner: &TypeInterner, ty: TypeId) -> bool {
    matches!(
        interner.kind(ty),
        TyKind::Void
            | TyKind::Prim(
                PrimTy::Int
                    | PrimTy::UInt
                    | PrimTy::Long
                    | PrimTy::ULong
                    | PrimTy::ISize
                    | PrimTy::USize
                    | PrimTy::Byte
                    | PrimTy::Bool
                    | PrimTy::Char
                    | PrimTy::Float16
                    | PrimTy::Float
                    | PrimTy::Double
            )
    )
}
