//! Constant folding: evaluate binary/unary operations whose operands are already constants and
//! replace them with the literal result.

use super::MirPass;
use crate::int_ty::IntTy;
use crate::{BinOp, Const, MirFunction, Operand, Place, Rvalue, Statement, UnOp};
use dream_types::TypeInterner;

pub struct ConstFold;

impl MirPass for ConstFold {
    fn name(&self) -> &'static str {
        "const-fold"
    }

    fn run(&self, func: &mut MirFunction, interner: &TypeInterner) -> bool {
        let mut changed = false;
        let locals = &func.locals;
        for block in &mut func.blocks {
            for stmt in &mut block.stmts {
                if let Statement::Assign(Place::Local(local), rvalue) = stmt {
                    let dest = IntTy::of(interner, locals[local.0 as usize].ty);
                    if let Some(folded) = fold(rvalue, dest) {
                        *rvalue = Rvalue::Use(Operand::Const(folded));
                        changed = true;
                    }
                }
            }
        }
        changed
    }
}

fn as_const(op: &Operand) -> Option<&Const> {
    match op {
        Operand::Const(c) => Some(c),
        _ => None,
    }
}

/// `dest` is the destination's integer type, which is the operation's type for arithmetic (MIR
/// constants carry width but not signedness; see [`crate::int_ty`]). Results are written back in
/// canonical form so later folds and the emitter read them the same way.
pub(super) fn fold(rvalue: &Rvalue, dest: Option<IntTy>) -> Option<Const> {
    match rvalue {
        Rvalue::Binary(op, a, b) => fold_binary(*op, as_const(a)?, as_const(b)?, dest),
        Rvalue::Unary(op, a) => fold_unary(*op, as_const(a)?, dest),
        Rvalue::CheckedBinary(op, a, b) => fold_checked(*op, as_const(a)?, as_const(b)?, dest?),
        Rvalue::CheckedNeg(a) => {
            let ty = dest?;
            let v = -ty.value(const_payload(as_const(a)?, ty)?);
            ty.fits(v).then(|| int_const(ty, v))
        }
        _ => None,
    }
}

/// The payload of an integer constant whose width matches `ty`.
fn const_payload(c: &Const, ty: IntTy) -> Option<i64> {
    match c {
        Const::Int(v) if !ty.is_64() => Some(*v),
        Const::Long(v) if ty.is_64() => Some(*v),
        _ => None,
    }
}

/// Folds a checked op only when it cannot panic; an overflowing constant expression is left in
/// place so it panics at run time like the same op on variables would.
fn fold_checked(op: BinOp, a: &Const, b: &Const, ty: IntTy) -> Option<Const> {
    let x = ty.value(const_payload(a, ty)?);
    let y = ty.value(const_payload(b, ty)?);
    let v = match op {
        BinOp::Add => x + y,
        BinOp::Sub => x - y,
        BinOp::Mul => x.checked_mul(y)?,
        BinOp::Div | BinOp::Rem if y == 0 => return None,
        BinOp::Div => x / y,
        BinOp::Rem if x == ty.min() && y == -1 => return None,
        BinOp::Rem => x % y,
        BinOp::Shl | BinOp::Shr if y < 0 || y >= ty.bits() as i128 => return None,
        BinOp::Shl => return Some(int_const(ty, x.wrapping_shl(y as u32))),
        BinOp::Shr => x >> y,
        _ => return None,
    };
    ty.fits(v).then(|| int_const(ty, v))
}

fn fold_binary(op: BinOp, a: &Const, b: &Const, dest: Option<IntTy>) -> Option<Const> {
    use Const::*;
    match (a, b) {
        (Int(x), Int(y)) => fold_int(op, *x, *y, false, dest),
        (Long(x), Long(y)) => fold_int(op, *x, *y, true, dest),
        (Float(x), Float(y)) => fold_float(op, *x, *y),
        (F32(x), F32(y)) => narrow_float(fold_float(op, *x as f64, *y as f64)?),
        (Bool(x), Bool(y)) => fold_bool(op, *x, *y),
        _ => None,
    }
}

fn int_const(ty: IntTy, v: i128) -> Const {
    if ty.is_64() {
        Const::Long(ty.wrap(v))
    } else {
        Const::Int(ty.wrap(v))
    }
}

/// Folds an integer operation on canonical payloads `px`/`py` (both 32-bit or both 64-bit, per
/// `wide`). Arithmetic wraps at the operation's type; division by zero is left to the runtime
/// panic.
fn fold_int(op: BinOp, px: i64, py: i64, wide: bool, dest: Option<IntTy>) -> Option<Const> {
    if op.is_comparison() {
        // A comparison's destination is `bool`, so signedness is unknown. Canonical 32-bit payloads
        // compare correctly as `i64` either way; a negative 64-bit payload may be a `ulong` at or
        // above 2^63, so leave that to the emitter.
        if wide && (px < 0 || py < 0) {
            return None;
        }
        return Some(Const::Bool(match op {
            BinOp::Eq => px == py,
            BinOp::Ne => px != py,
            BinOp::Lt => px < py,
            BinOp::Le => px <= py,
            BinOp::Gt => px > py,
            _ => px >= py,
        }));
    }
    let ty = match dest {
        Some(ty) if ty.is_64() == wide => ty,
        Some(_) => return None,
        None if wide => IntTy::Long,
        None => IntTy::Int,
    };
    let (x, y) = (ty.value(px), ty.value(py));
    let shift = (py as u32) & (ty.bits() - 1);
    let v = match op {
        BinOp::Add => x.wrapping_add(y),
        BinOp::Sub => x.wrapping_sub(y),
        BinOp::Mul => x.wrapping_mul(y),
        BinOp::Div if y != 0 => x / y,
        BinOp::Rem if y != 0 => x % y,
        BinOp::BitAnd => x & y,
        BinOp::BitOr => x | y,
        BinOp::BitXor => x ^ y,
        BinOp::Shl => x.wrapping_shl(shift),
        BinOp::Shr => x >> shift,
        _ => return None,
    };
    Some(int_const(ty, v))
}

/// Re-narrows an f64 fold result to [`Const::F32`] (comparisons pass through as `Bool`) so
/// `float`+`float` stays `float`.
fn narrow_float(folded: Const) -> Option<Const> {
    Some(match folded {
        Const::Float(v) => Const::F32(v as f32),
        other => other,
    })
}

fn fold_float(op: BinOp, x: f64, y: f64) -> Option<Const> {
    Some(match op {
        BinOp::Add => Const::Float(x + y),
        BinOp::Sub => Const::Float(x - y),
        BinOp::Mul => Const::Float(x * y),
        BinOp::Div => Const::Float(x / y),
        BinOp::Eq => Const::Bool(x == y),
        BinOp::Ne => Const::Bool(x != y),
        BinOp::Lt => Const::Bool(x < y),
        BinOp::Le => Const::Bool(x <= y),
        BinOp::Gt => Const::Bool(x > y),
        BinOp::Ge => Const::Bool(x >= y),
        _ => return None,
    })
}

fn fold_bool(op: BinOp, x: bool, y: bool) -> Option<Const> {
    Some(match op {
        BinOp::And => Const::Bool(x && y),
        BinOp::Or => Const::Bool(x || y),
        BinOp::Eq => Const::Bool(x == y),
        BinOp::Ne => Const::Bool(x != y),
        _ => return None,
    })
}

fn fold_unary(op: UnOp, a: &Const, dest: Option<IntTy>) -> Option<Const> {
    let int_ty = |wide: bool| match dest {
        Some(ty) if ty.is_64() == wide => Some(ty),
        Some(_) => None,
        None if wide => Some(IntTy::Long),
        None => Some(IntTy::Int),
    };
    Some(match (op, a) {
        (UnOp::Neg, Const::Int(x)) => {
            let ty = int_ty(false)?;
            int_const(ty, -ty.value(*x))
        }
        (UnOp::Neg, Const::Long(x)) => {
            let ty = int_ty(true)?;
            int_const(ty, -ty.value(*x))
        }
        (UnOp::BitNot, Const::Int(x)) => {
            let ty = int_ty(false)?;
            int_const(ty, !ty.value(*x))
        }
        (UnOp::BitNot, Const::Long(x)) => {
            let ty = int_ty(true)?;
            int_const(ty, !ty.value(*x))
        }
        (UnOp::Neg, Const::Float(x)) => Const::Float(-x),
        (UnOp::Neg, Const::F32(x)) => Const::F32(-x),
        (UnOp::Not, Const::Bool(x)) => Const::Bool(!x),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;
    use crate::{Operand, Place, Rvalue, Terminator};
    use dream_types::TypeInterner;

    fn bin(op: BinOp, a: Const, b: Const) -> Rvalue {
        Rvalue::Binary(op, Operand::Const(a), Operand::Const(b))
    }

    #[test]
    fn folds_int_add() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("f", i.int());
        let t = b.new_temp(i.int());
        b.assign(
            Place::Local(t),
            bin(BinOp::Add, Const::Int(2), Const::Int(3)),
        );
        b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
        let mut func = b.finish();
        assert!(ConstFold.run(&mut func, &i));
        match &func.blocks[0].stmts[0] {
            Statement::Assign(_, Rvalue::Use(Operand::Const(Const::Int(v)))) => assert_eq!(*v, 5),
            other => panic!("expected folded const, got {:?}", other),
        }
    }

    #[test]
    fn uint_results_stay_canonical() {
        let uint = Some(IntTy::UInt);
        assert_eq!(
            fold(
                &bin(BinOp::Add, Const::Int(4294967295), Const::Int(1)),
                uint
            ),
            Some(Const::Int(0))
        );
        assert_eq!(
            fold(&bin(BinOp::Sub, Const::Int(0), Const::Int(1)), uint),
            Some(Const::Int(4294967295))
        );
        assert_eq!(
            fold(
                &bin(BinOp::Div, Const::Int(4294967295), Const::Int(2)),
                uint
            ),
            Some(Const::Int(2147483647))
        );
        assert_eq!(
            fold(&bin(BinOp::Gt, Const::Int(4294967295), Const::Int(1)), None),
            Some(Const::Bool(true))
        );
    }

    #[test]
    fn int_wraps_at_32_bits() {
        let int = Some(IntTy::Int);
        assert_eq!(
            fold(&bin(BinOp::Add, Const::Int(2147483647), Const::Int(1)), int),
            Some(Const::Int(-2147483648))
        );
        assert_eq!(
            fold(
                &bin(BinOp::Div, Const::Int(-2147483648), Const::Int(-1)),
                int
            ),
            Some(Const::Int(-2147483648))
        );
        assert_eq!(
            fold(&bin(BinOp::Shl, Const::Int(1), Const::Int(33)), int),
            Some(Const::Int(2))
        );
    }

    #[test]
    fn long_wraps_at_64_bits_and_ulong_is_unsigned() {
        assert_eq!(
            fold(
                &bin(BinOp::Add, Const::Long(i64::MAX), Const::Long(1)),
                Some(IntTy::Long)
            ),
            Some(Const::Long(i64::MIN))
        );
        assert_eq!(
            fold(
                &bin(BinOp::Shr, Const::Long(-1), Const::Long(60)),
                Some(IntTy::ULong)
            ),
            Some(Const::Long(15))
        );
        assert_eq!(
            fold(&bin(BinOp::Lt, Const::Long(-1), Const::Long(1)), None),
            None
        );
    }

    #[test]
    fn byte_masks_into_0_255() {
        let rv = bin(BinOp::Add, Const::Int(250), Const::Int(10));
        assert_eq!(fold(&rv, Some(IntTy::Byte)), Some(Const::Int(4)));
        assert_eq!(
            fold(
                &Rvalue::Unary(UnOp::BitNot, Operand::Const(Const::Int(0))),
                Some(IntTy::Byte)
            ),
            Some(Const::Int(255))
        );
    }

    #[test]
    fn checked_folds_only_when_the_result_fits() {
        let checked = |op, a, b, ty| {
            fold(
                &Rvalue::CheckedBinary(op, Operand::Const(a), Operand::Const(b)),
                Some(ty),
            )
        };
        assert_eq!(
            checked(BinOp::Add, Const::Int(2), Const::Int(3), IntTy::Int),
            Some(Const::Int(5))
        );
        assert_eq!(
            checked(
                BinOp::Add,
                Const::Int(2147483647),
                Const::Int(1),
                IntTy::Int
            ),
            None
        );
        assert_eq!(
            checked(BinOp::Sub, Const::Int(0), Const::Int(1), IntTy::UInt),
            None
        );
        assert_eq!(
            checked(BinOp::Mul, Const::Int(16), Const::Int(16), IntTy::Byte),
            None
        );
        assert_eq!(
            checked(
                BinOp::Div,
                Const::Int(-2147483648),
                Const::Int(-1),
                IntTy::Int
            ),
            None
        );
        assert_eq!(
            checked(BinOp::Shl, Const::Int(1), Const::Int(32), IntTy::Int),
            None
        );
        assert_eq!(
            checked(BinOp::Shl, Const::Int(3), Const::Int(31), IntTy::Int),
            Some(Const::Int(-2147483648))
        );
        assert_eq!(
            fold(
                &Rvalue::CheckedNeg(Operand::Const(Const::Long(i64::MIN))),
                Some(IntTy::Long)
            ),
            None
        );
    }

    #[test]
    fn division_by_zero_is_left_for_runtime() {
        assert_eq!(
            fold(
                &bin(BinOp::Div, Const::Int(1), Const::Int(0)),
                Some(IntTy::Int)
            ),
            None
        );
    }
}
