//! Integer arithmetic with Dream's defined semantics: every op wraps at its type's width, unsigned
//! types compare/divide/shift unsigned, shift counts are masked to the width, and division by zero
//! panics. Plain C `+`/`<<`/`/` on signed operands is undefined on overflow and native `int`
//! locals are 64-bit, so every op is spelled through the fixed-width unsigned type and narrowed
//! back to the op's type.

use super::ast::{CTy, Expr};
use super::emit::Emitter;
use crate::backend::shared::panic_msgs as msgs;
use crate::int_ty::IntTy;
use crate::{BinOp, Operand, UnOp};
use dream_types::TypeId;

fn signed_c(ty: IntTy) -> CTy {
    match ty {
        IntTy::Byte => CTy::U8,
        IntTy::Int => CTy::I32,
        IntTy::UInt => CTy::U32,
        IntTy::Long => CTy::I64,
        IntTy::ULong => CTy::U64,
    }
}

fn unsigned_c(ty: IntTy) -> CTy {
    match ty {
        IntTy::Byte => CTy::U8,
        IntTy::Int | IntTy::UInt => CTy::U32,
        IntTy::Long | IntTy::ULong => CTy::U64,
    }
}

fn const_int_ty(o: &Operand) -> Option<IntTy> {
    match o {
        Operand::Const(crate::Const::Int(_)) => Some(IntTy::Int),
        Operand::Const(crate::Const::Long(_)) => Some(IntTy::Long),
        _ => None,
    }
}

impl<'a> Emitter<'a> {
    fn operand_int_ty(&self, o: &Operand) -> Option<IntTy> {
        match o {
            Operand::Const(_) => const_int_ty(o),
            _ => IntTy::of(self.cx.interner, self.operand_ty(o)),
        }
    }

    /// The type an integer binary op is performed at, or `None` for non-integer operands.
    /// Arithmetic takes the destination's type (constants carry no signedness); a comparison takes
    /// its non-constant operand's type.
    pub(super) fn binary_int_ty(
        &self,
        op: BinOp,
        a: &Operand,
        b: &Operand,
        dest: Option<TypeId>,
    ) -> Option<IntTy> {
        let operand = match (a, b) {
            (Operand::Const(_), Operand::Const(_)) => self.operand_int_ty(a),
            (Operand::Const(_), _) => self.operand_int_ty(b),
            _ => self.operand_int_ty(a),
        }?;
        if op.is_comparison() || matches!(op, BinOp::And | BinOp::Or) {
            return Some(operand);
        }
        let dest = dest.and_then(|t| IntTy::of(self.cx.interner, t));
        Some(match dest {
            Some(d) if d.is_64() == operand.is_64() => d,
            _ => operand,
        })
    }

    pub(super) fn unary_int_ty(&self, a: &Operand, dest: Option<TypeId>) -> Option<IntTy> {
        let operand = self.operand_int_ty(a)?;
        let dest = dest.and_then(|t| IntTy::of(self.cx.interner, t));
        Some(match dest {
            Some(d) if d.is_64() == operand.is_64() => d,
            _ => operand,
        })
    }

    pub(super) fn int_binary(&mut self, op: BinOp, ty: IntTy, a: &Operand, b: &Operand) -> Expr {
        let (s, u) = (signed_c(ty), unsigned_c(ty));
        let lhs = self.operand(a);
        let rhs = self.operand(b);
        let wrap = |op, l: Expr, r: Expr| {
            Expr::cast(
                s.clone(),
                Expr::bin(op, Expr::cast(u.clone(), l), Expr::cast(u.clone(), r)),
            )
        };
        let count = |r: Expr| {
            Expr::bin(
                BinOp::BitAnd,
                Expr::cast(u.clone(), r),
                Expr::i(ty.bits() as i64 - 1),
            )
        };
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                wrap(op, lhs, rhs)
            }
            BinOp::Shl => Expr::cast(
                s.clone(),
                Expr::bin(BinOp::Shl, Expr::cast(u.clone(), lhs), count(rhs)),
            ),
            BinOp::Shr => Expr::cast(
                s.clone(),
                Expr::bin(BinOp::Shr, Expr::cast(s.clone(), lhs), count(rhs)),
            ),
            BinOp::Div | BinOp::Rem => self.int_div(op, ty, lhs, rhs),
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
                if !ty.signed() =>
            {
                Expr::bin(op, Expr::cast(s.clone(), lhs), Expr::cast(s, rhs))
            }
            _ => Expr::bin(op, lhs, rhs),
        }
    }

    /// Division panics on a zero divisor; signed `MIN / -1` wraps to `MIN` (and `MIN % -1` is 0)
    /// instead of trapping, like every other overflowing op.
    fn int_div(&mut self, op: BinOp, ty: IntTy, lhs: Expr, rhs: Expr) -> Expr {
        let (s, u) = (signed_c(ty), unsigned_c(ty));
        let panic = Expr::id(self.cx.str_sym(msgs::DIVIDE_BY_ZERO));
        self.b.expr_block(move |b| {
            let x = b.temp(s.clone(), Some(Expr::cast(s.clone(), lhs)));
            let y = b.temp(s.clone(), Some(Expr::cast(s.clone(), rhs)));
            let quotient = Expr::cast(s.clone(), Expr::bin(op, x.clone(), y.clone()));
            let value = if ty.signed() {
                let by_minus_one = if op == BinOp::Div {
                    Expr::cast(
                        s.clone(),
                        Expr::bin(BinOp::Sub, Expr::i(0), Expr::cast(u, x.clone())),
                    )
                } else {
                    Expr::cast(s.clone(), Expr::i(0))
                };
                Expr::ternary(Expr::eq(y.clone(), Expr::i(-1)), by_minus_one, quotient)
            } else {
                quotient
            };
            Expr::ternary(
                Expr::eq(y, Expr::i(0)),
                Expr::comma(
                    Expr::call("dream_panic", vec![panic]),
                    Expr::cast(s, Expr::i(0)),
                ),
                value,
            )
        })
    }

    /// `CheckedBinary`: `+ - *` go through `__builtin_*_overflow` into a temporary of the exact
    /// type; signed `/ %` panic on `MIN / -1`; shifts panic on a count outside `0..bits` (bits
    /// shifted off the top are not an overflow).
    pub(super) fn checked_binary(
        &mut self,
        op: BinOp,
        ty: IntTy,
        a: &Operand,
        b: &Operand,
    ) -> Expr {
        let (s, u) = (signed_c(ty), unsigned_c(ty));
        let lhs = self.operand(a);
        let rhs = self.operand(b);
        let (builtin, msg) = match op {
            BinOp::Add => ("__builtin_add_overflow", msgs::ADD_OVERFLOW),
            BinOp::Sub => ("__builtin_sub_overflow", msgs::SUB_OVERFLOW),
            BinOp::Mul => ("__builtin_mul_overflow", msgs::MUL_OVERFLOW),
            BinOp::Div => ("", msgs::DIV_OVERFLOW),
            BinOp::Rem => ("", msgs::REM_OVERFLOW),
            BinOp::Shl => ("", msgs::SHL_OVERFLOW),
            BinOp::Shr => ("", msgs::SHR_OVERFLOW),
            _ => crate::internal_error!("checked op {op:?} has no overflow check"),
        };
        let overflow = Expr::id(self.cx.str_sym(msg));
        let panic_with = move |msg: Expr, s: CTy| {
            Expr::comma(
                Expr::call("dream_panic", vec![msg]),
                Expr::cast(s, Expr::i(0)),
            )
        };
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul => self.b.expr_block(move |b| {
                let r = b.temp(s.clone(), None);
                Expr::ternary(
                    Expr::call(
                        builtin,
                        vec![
                            Expr::cast(s.clone(), lhs),
                            Expr::cast(s.clone(), rhs),
                            Expr::addr_of(r.clone()),
                        ],
                    ),
                    panic_with(overflow, s),
                    r,
                )
            }),
            BinOp::Div | BinOp::Rem => {
                let zero = Expr::id(self.cx.str_sym(msgs::DIVIDE_BY_ZERO));
                self.b.expr_block(move |b| {
                    let x = b.temp(s.clone(), Some(Expr::cast(s.clone(), lhs)));
                    let y = b.temp(s.clone(), Some(Expr::cast(s.clone(), rhs)));
                    let min = Expr::id(if ty.is_64() { "INT64_MIN" } else { "INT32_MIN" });
                    Expr::ternary(
                        Expr::eq(y.clone(), Expr::i(0)),
                        panic_with(zero, s.clone()),
                        Expr::ternary(
                            Expr::and(Expr::eq(x.clone(), min), Expr::eq(y.clone(), Expr::i(-1))),
                            panic_with(overflow, s.clone()),
                            Expr::cast(s, Expr::bin(op, x, y)),
                        ),
                    )
                })
            }
            _ => self.b.expr_block(move |b| {
                let x = b.temp(s.clone(), Some(Expr::cast(s.clone(), lhs)));
                let count = b.temp(u.clone(), Some(Expr::cast(u.clone(), rhs)));
                let shifted = if op == BinOp::Shl {
                    Expr::bin(BinOp::Shl, Expr::cast(u, x), count.clone())
                } else {
                    Expr::bin(BinOp::Shr, x, count.clone())
                };
                Expr::ternary(
                    Expr::bin(BinOp::Ge, count, Expr::i(ty.bits() as i64)),
                    panic_with(overflow, s.clone()),
                    Expr::cast(s, shifted),
                )
            }),
        }
    }

    pub(super) fn checked_neg(&mut self, ty: IntTy, a: &Operand) -> Expr {
        let s = signed_c(ty);
        let v = self.operand(a);
        let overflow = Expr::id(self.cx.str_sym(msgs::NEG_OVERFLOW));
        self.b.expr_block(move |b| {
            let r = b.temp(s.clone(), None);
            Expr::ternary(
                Expr::call(
                    "__builtin_sub_overflow",
                    vec![
                        Expr::cast(s.clone(), Expr::i(0)),
                        Expr::cast(s.clone(), v),
                        Expr::addr_of(r.clone()),
                    ],
                ),
                Expr::comma(
                    Expr::call("dream_panic", vec![overflow]),
                    Expr::cast(s, Expr::i(0)),
                ),
                r,
            )
        })
    }

    pub(super) fn int_unary(&mut self, op: UnOp, ty: IntTy, a: &Operand) -> Expr {
        let (s, u) = (signed_c(ty), unsigned_c(ty));
        let v = Expr::cast(u, self.operand(a));
        match op {
            UnOp::Neg => Expr::cast(s, Expr::bin(BinOp::Sub, Expr::i(0), v)),
            _ => Expr::cast(s, Expr::unary(super::ast::UnOp::BitNot, v)),
        }
    }
}
