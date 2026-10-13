use crate::{Const, MirFunction, Operand, Place};
use dream_types::{PrimTy, TyKind, TypeInterner};

// String operands are heap reads despite their primitive type spelling. Arithmetic passes
// must not reuse their contents across writes or speculate allocating concatenations.
pub(super) fn local_flags(func: &MirFunction, interner: &TypeInterner) -> Vec<bool> {
    func.locals
        .iter()
        .map(|local| matches!(interner.kind(local.ty), TyKind::Prim(p) if *p != PrimTy::String))
        .collect()
}

pub(super) fn operand_is_scalar(operand: &Operand, locals: &[bool]) -> bool {
    match operand {
        Operand::Copy(Place::Local(l)) => locals[l.0 as usize],
        Operand::Const(c) => matches!(
            c,
            Const::Int(_)
                | Const::Long(_)
                | Const::Float(_)
                | Const::F32(_)
                | Const::Bool(_)
                | Const::Char(_)
        ),
        _ => false,
    }
}
