//! Integer arithmetic with Dream's defined semantics: every op wraps at its type's width
//! (plain `add`/`mul`, never `nsw`/`nuw`), unsigned types compare/divide/shift unsigned, shift
//! counts are masked to the width, division by zero panics, and signed `MIN / -1` wraps. Checked
//! ops use LLVM's overflow intrinsics and panic instead of wrapping.

use super::fx::{Fx, V};
use super::ir::{FnTy, Ty, Value};
use crate::backend::shared::panic_msgs as msgs;
use crate::int_ty::IntTy;
use crate::{BinOp, Operand, UnOp};
use dream_types::TypeId;

fn int_ll(ty: IntTy) -> Ty {
    Ty::Int(ty.bits())
}

fn const_int_ty(o: &Operand) -> Option<IntTy> {
    match o {
        Operand::Const(crate::Const::Int(_)) => Some(IntTy::Int),
        Operand::Const(crate::Const::Long(_)) => Some(IntTy::Long),
        _ => None,
    }
}

fn icmp_pred(op: BinOp, signed: bool) -> &'static str {
    match (op, signed) {
        (BinOp::Eq, _) => "eq",
        (BinOp::Ne, _) => "ne",
        (BinOp::Lt, true) => "slt",
        (BinOp::Le, true) => "sle",
        (BinOp::Gt, true) => "sgt",
        (BinOp::Ge, true) => "sge",
        (BinOp::Lt, false) => "ult",
        (BinOp::Le, false) => "ule",
        (BinOp::Gt, false) => "ugt",
        (BinOp::Ge, false) => "uge",
        _ => crate::internal_error!("{op:?} is not a comparison"),
    }
}

impl<'l, 'a> Fx<'l, 'a> {
    fn operand_int_ty(&self, o: &Operand) -> Option<IntTy> {
        match o {
            Operand::Const(_) => const_int_ty(o),
            _ => IntTy::of(
                self.interner,
                self.operand_ty(o),
                self.l.cx.mir.layouts.target.ptr_size,
            ),
        }
    }

    /// The type an integer binary op is performed at, or `None` for non-integer operands.
    /// Arithmetic takes the destination's type (constants carry no signedness); a comparison takes
    /// its non-constant operand's type.
    pub fn binary_int_ty(
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
        let dest =
            dest.and_then(|t| IntTy::of(self.interner, t, self.l.cx.mir.layouts.target.ptr_size));
        Some(match dest {
            Some(d) if d.is_64() == operand.is_64() => d,
            _ => operand,
        })
    }

    pub fn unary_int_ty(&self, a: &Operand, dest: Option<TypeId>) -> Option<IntTy> {
        let operand = self.operand_int_ty(a)?;
        let dest =
            dest.and_then(|t| IntTy::of(self.interner, t, self.l.cx.mir.layouts.target.ptr_size));
        Some(match dest {
            Some(d) if d.is_64() == operand.is_64() => d,
            _ => operand,
        })
    }

    fn at(&mut self, x: &V, ty: IntTy) -> Value {
        self.conv(x, &int_ll(ty))
    }

    fn int_result(v: Value, ty: IntTy) -> V {
        V {
            v,
            unsigned: !ty.signed(),
        }
    }

    pub fn bool_v(&mut self, c: &Value) -> V {
        V::s(self.conv(&V::u(c.clone()), &Ty::I32))
    }

    pub fn int_binary(&mut self, op: BinOp, ty: IntTy, a: &Operand, b: &Operand) -> V {
        let lhs = self.operand(a);
        let rhs = self.operand(b);
        let mask = Value::int(int_ll(ty), ty.bits() as i128 - 1);
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                let name = match op {
                    BinOp::Add => "add",
                    BinOp::Sub => "sub",
                    BinOp::Mul => "mul",
                    BinOp::BitAnd => "and",
                    BinOp::BitOr => "or",
                    _ => "xor",
                };
                let (l, r) = (self.at(&lhs, ty), self.at(&rhs, ty));
                Self::int_result(self.w.bin(name, &l, &r), ty)
            }
            BinOp::Shl | BinOp::Shr => {
                let l = self.at(&lhs, ty);
                let r = self.at(&rhs, ty);
                let count = self.w.bin("and", &r, &mask);
                let name = match (op, ty.signed()) {
                    (BinOp::Shl, _) => "shl",
                    (_, true) => "ashr",
                    _ => "lshr",
                };
                Self::int_result(self.w.bin(name, &l, &count), ty)
            }
            BinOp::Div | BinOp::Rem => self.int_div(op, ty, &lhs, &rhs),
            _ if op.is_comparison() && !ty.signed() => {
                let (l, r) = (self.at(&lhs, ty), self.at(&rhs, ty));
                let c = self.w.icmp(icmp_pred(op, false), &l, &r);
                self.bool_v(&c)
            }
            _ => self.c_binary(op, &lhs, &rhs),
        }
    }

    pub(super) fn panic_if(&mut self, cond: &Value, msg: &str) {
        let bad = self.w.new_block("panic");
        let ok = self.w.new_block("ok");
        self.w.cond_br(cond, bad, ok);
        self.w.switch_to(bad);
        self.panic_with(msg);
        if !self.w.is_terminated() {
            self.w.unreachable();
        }
        self.w.switch_to(ok);
    }

    /// Division panics on a zero divisor; signed `MIN / -1` wraps to `MIN` (and `MIN % -1` is 0).
    fn int_div(&mut self, op: BinOp, ty: IntTy, lhs: &V, rhs: &V) -> V {
        let t = int_ll(ty);
        let x = self.at(lhs, ty);
        let y = self.at(rhs, ty);
        let width_mask = u128::MAX >> (128 - ty.bits());
        let known = y.const_int().map(|v| v as u128 & width_mask);
        if known.is_none_or(|v| v == 0) {
            let zero = self.w.icmp("eq", &y, &Value::zero(t.clone()));
            self.panic_if(&zero, msgs::DIVIDE_BY_ZERO);
        }
        if !ty.signed() {
            let name = if op == BinOp::Div { "udiv" } else { "urem" };
            return Self::int_result(self.w.bin(name, &x, &y), ty);
        }
        if known.is_some_and(|v| v != 0 && v != width_mask) {
            let name = if op == BinOp::Div { "sdiv" } else { "srem" };
            return Self::int_result(self.w.bin(name, &x, &y), ty);
        }
        let m1 = self.w.icmp("eq", &y, &Value::int(t.clone(), -1));
        let safe = self.w.select(&m1, &Value::int(t.clone(), 1), &y);
        let name = if op == BinOp::Div { "sdiv" } else { "srem" };
        let q = self.w.bin(name, &x, &safe);
        let alt = if op == BinOp::Div {
            self.w.bin("sub", &Value::zero(t.clone()), &x)
        } else {
            Value::zero(t)
        };
        Self::int_result(self.w.select(&m1, &alt, &q), ty)
    }

    fn overflow_op(
        &mut self,
        base: &str,
        signed: bool,
        t: &Ty,
        l: &Value,
        r: &Value,
        msg: &str,
    ) -> Value {
        let name = format!(
            "llvm.{}{base}.with.overflow.{t}",
            if signed { "s" } else { "u" }
        );
        let ret = Ty::Struct {
            packed: false,
            fields: vec![t.clone(), Ty::I1],
        };
        let sig = self
            .l
            .intrinsic(&name, FnTy::new(ret.clone(), vec![t.clone(), t.clone()]));
        let pair = self
            .call_ptr(
                &Value::global(name.clone()),
                &sig,
                vec![l.clone(), r.clone()],
            )
            .unwrap_or_else(|| crate::internal_error!("{name} returned void"));
        let v = self.w.extract(&pair, 0, t.clone());
        let o = self.w.extract(&pair, 1, Ty::I1);
        self.panic_if(&o, msg);
        v
    }

    /// `CheckedBinary`: `+ - *` panic when the exact result leaves the type; signed `/ %` panic on
    /// `MIN / -1`; shifts panic on a count outside `0..bits` (bits shifted off the top are fine).
    pub fn checked_binary(&mut self, op: BinOp, ty: IntTy, a: &Operand, b: &Operand) -> V {
        let t = int_ll(ty);
        let lhs = self.operand(a);
        let rhs = self.operand(b);
        let x = self.at(&lhs, ty);
        let y = self.at(&rhs, ty);
        let v = match op {
            BinOp::Add => self.overflow_op("add", ty.signed(), &t, &x, &y, msgs::ADD_OVERFLOW),
            BinOp::Sub => self.overflow_op("sub", ty.signed(), &t, &x, &y, msgs::SUB_OVERFLOW),
            BinOp::Mul => self.overflow_op("mul", ty.signed(), &t, &x, &y, msgs::MUL_OVERFLOW),
            BinOp::Div | BinOp::Rem => {
                let zero = self.w.icmp("eq", &y, &Value::zero(t.clone()));
                self.panic_if(&zero, msgs::DIVIDE_BY_ZERO);
                let signed = ty.signed();
                if signed {
                    let min = Value::int(t.clone(), -(1i128 << (ty.bits() - 1)));
                    let is_min = self.w.icmp("eq", &x, &min);
                    let m1 = self.w.icmp("eq", &y, &Value::int(t.clone(), -1));
                    let both = self.w.bin("and", &is_min, &m1);
                    let msg = if op == BinOp::Div {
                        msgs::DIV_OVERFLOW
                    } else {
                        msgs::REM_OVERFLOW
                    };
                    self.panic_if(&both, msg);
                }
                let name = match (op, signed) {
                    (BinOp::Div, true) => "sdiv",
                    (BinOp::Div, false) => "udiv",
                    (_, true) => "srem",
                    _ => "urem",
                };
                self.w.bin(name, &x, &y)
            }
            BinOp::Shl | BinOp::Shr => {
                let bits = Value::int(t.clone(), ty.bits() as i128);
                let too_far = self.w.icmp("uge", &y, &bits);
                let msg = if op == BinOp::Shl {
                    msgs::SHL_OVERFLOW
                } else {
                    msgs::SHR_OVERFLOW
                };
                self.panic_if(&too_far, msg);
                let name = match (op, ty.signed()) {
                    (BinOp::Shl, _) => "shl",
                    (_, true) => "ashr",
                    _ => "lshr",
                };
                self.w.bin(name, &x, &y)
            }
            _ => crate::internal_error!("checked op {op:?} has no overflow check"),
        };
        Self::int_result(v, ty)
    }

    pub fn checked_neg(&mut self, ty: IntTy, a: &Operand) -> V {
        let t = int_ll(ty);
        let v = self.operand(a);
        let x = self.at(&v, ty);
        let r = self.overflow_op(
            "sub",
            ty.signed(),
            &t,
            &Value::zero(t.clone()),
            &x,
            msgs::NEG_OVERFLOW,
        );
        Self::int_result(r, ty)
    }

    pub fn int_unary(&mut self, op: UnOp, ty: IntTy, a: &Operand) -> V {
        let t = int_ll(ty);
        let v = self.operand(a);
        let x = self.at(&v, ty);
        let r = match op {
            UnOp::Neg => self.w.bin("sub", &Value::zero(t), &x),
            _ => self.w.bin("xor", &x, &Value::int(t, -1)),
        };
        Self::int_result(r, ty)
    }

    /// C's usual arithmetic conversions for a binary operator on two already-evaluated operands.
    fn common(&mut self, l: &V, r: &V) -> (Value, Value, bool, bool) {
        let float_rank = |t: &Ty| match t {
            Ty::F64 => 2,
            Ty::F32 => 1,
            _ => 0,
        };
        let (fl, fr) = (float_rank(l.ty()), float_rank(r.ty()));
        if fl > 0 || fr > 0 {
            let t = if fl.max(fr) == 2 { Ty::F64 } else { Ty::F32 };
            return (self.conv(l, &t), self.conv(r, &t), false, true);
        }
        let bits = |t: &Ty| t.int_bits().unwrap_or(64);
        let promote = |x: &V| -> (u32, bool) {
            let b = bits(x.ty());
            if b < 32 {
                (32, false)
            } else {
                (b, x.unsigned)
            }
        };
        let (bl, ul) = promote(l);
        let (br, ur) = promote(r);
        let (w, unsigned) = match bl.cmp(&br) {
            std::cmp::Ordering::Equal => (bl, ul || ur),
            std::cmp::Ordering::Greater => (bl, ul),
            std::cmp::Ordering::Less => (br, ur),
        };
        let t = Ty::Int(w);
        (self.conv(l, &t), self.conv(r, &t), unsigned, false)
    }

    /// `l op r` exactly as C evaluates it on the operands' C types.
    pub fn c_binary(&mut self, op: BinOp, l: &V, r: &V) -> V {
        if matches!(op, BinOp::And | BinOp::Or) {
            let (a, b) = (self.truthy(l), self.truthy(r));
            let c = self
                .w
                .bin(if op == BinOp::And { "and" } else { "or" }, &a, &b);
            return self.bool_v(&c);
        }
        let (a, b, unsigned, float) = self.common(l, r);
        if op.is_comparison() {
            let c = if float {
                let pred = match op {
                    BinOp::Eq => "oeq",
                    BinOp::Ne => "une",
                    BinOp::Lt => "olt",
                    BinOp::Le => "ole",
                    BinOp::Gt => "ogt",
                    _ => "oge",
                };
                self.w.fcmp(pred, &a, &b)
            } else {
                self.w.icmp(icmp_pred(op, !unsigned), &a, &b)
            };
            return self.bool_v(&c);
        }
        let name = match (op, float, unsigned) {
            (BinOp::Add, true, _) => "fadd",
            (BinOp::Sub, true, _) => "fsub",
            (BinOp::Mul, true, _) => "fmul",
            (BinOp::Div, true, _) => "fdiv",
            (BinOp::Rem, true, _) => "frem",
            (BinOp::Add, false, _) => "add",
            (BinOp::Sub, false, _) => "sub",
            (BinOp::Mul, false, _) => "mul",
            (BinOp::BitAnd, false, _) => "and",
            (BinOp::BitOr, false, _) => "or",
            (BinOp::BitXor, false, _) => "xor",
            (BinOp::Shl, false, _) => "shl",
            (BinOp::Shr, false, true) => "lshr",
            (BinOp::Shr, false, false) => "ashr",
            (BinOp::Div, false, true) => "udiv",
            (BinOp::Div, false, false) => "sdiv",
            (BinOp::Rem, false, true) => "urem",
            (BinOp::Rem, false, false) => "srem",
            _ => crate::internal_error!("{op:?} on float operands"),
        };
        V {
            v: self.w.bin(name, &a, &b),
            unsigned,
        }
    }
}
