//! Integer signedness and width, shared by constant folding and the C emitter so both agree on
//! how an operation wraps, divides, shifts, and compares.
//!
//! MIR constants do not record signedness: [`crate::Const::Int`] covers `int`/`uint`/`byte` and
//! [`crate::Const::Long`] covers `long`/`ulong`. The type of an arithmetic operation therefore
//! comes from its destination (or a non-constant operand), and constants are kept in *canonical*
//! form — the value's true magnitude for `uint`/`byte`, sign-extended for signed types, and the raw
//! bit pattern for `ulong` — so a comparison of two constants is correct as a plain `i64` compare
//! in every case except `ulong` values at or above `2^63`.

use dream_types::{PrimTy, TyKind, TypeId, TypeInterner};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntTy {
    Byte,
    Int,
    UInt,
    Long,
    ULong,
}

impl IntTy {
    /// The integer type of `ty`, if it has integer arithmetic. C-style enums and `char` are
    /// 32-bit signed values at runtime.
    pub fn of(interner: &TypeInterner, ty: TypeId) -> Option<IntTy> {
        match interner.kind(ty) {
            TyKind::Prim(PrimTy::Byte) => Some(IntTy::Byte),
            TyKind::Prim(PrimTy::Int | PrimTy::Char) | TyKind::Enum(_) => Some(IntTy::Int),
            TyKind::Prim(PrimTy::UInt) => Some(IntTy::UInt),
            TyKind::Prim(PrimTy::Long) => Some(IntTy::Long),
            TyKind::Prim(PrimTy::ULong) => Some(IntTy::ULong),
            _ => None,
        }
    }

    /// Like [`IntTy::of`] but only for the integer primitives, which are the types whose
    /// arithmetic is overflow-checked.
    pub fn of_prim(interner: &TypeInterner, ty: TypeId) -> Option<IntTy> {
        match interner.kind(ty) {
            TyKind::Prim(_) => IntTy::of(interner, ty)
                .filter(|_| !matches!(interner.kind(ty), TyKind::Prim(PrimTy::Char))),
            _ => None,
        }
    }

    pub fn bits(self) -> u32 {
        match self {
            IntTy::Byte => 8,
            IntTy::Int | IntTy::UInt => 32,
            IntTy::Long | IntTy::ULong => 64,
        }
    }

    pub fn signed(self) -> bool {
        matches!(self, IntTy::Int | IntTy::Long)
    }

    pub fn is_64(self) -> bool {
        self.bits() == 64
    }

    pub fn min(self) -> i128 {
        if self.signed() {
            -(1i128 << (self.bits() - 1))
        } else {
            0
        }
    }

    pub fn max(self) -> i128 {
        if self.signed() {
            (1i128 << (self.bits() - 1)) - 1
        } else {
            (1i128 << self.bits()) - 1
        }
    }

    /// Interprets a constant payload as this type's mathematical value. Masking by width makes
    /// this robust to non-canonical payloads (e.g. a `uint` written as `-1`).
    pub fn value(self, payload: i64) -> i128 {
        match self {
            IntTy::Byte => (payload as u8) as i128,
            IntTy::Int => (payload as i32) as i128,
            IntTy::UInt => (payload as u32) as i128,
            IntTy::Long => payload as i128,
            IntTy::ULong => (payload as u64) as i128,
        }
    }

    /// Wraps a mathematical value into this type and returns its canonical constant payload.
    pub fn wrap(self, v: i128) -> i64 {
        match self {
            IntTy::Byte => (v as u8) as i64,
            IntTy::Int => (v as i32) as i64,
            IntTy::UInt => (v as u32) as i64,
            IntTy::Long => v as i64,
            IntTy::ULong => (v as u64) as i64,
        }
    }

    /// True when `v` is representable without wrapping.
    pub fn fits(self, v: i128) -> bool {
        v >= self.min() && v <= self.max()
    }
}

#[cfg(test)]
mod tests {
    use super::IntTy;

    #[test]
    fn canonical_payloads() {
        assert_eq!(IntTy::UInt.wrap(-1), 4_294_967_295);
        assert_eq!(IntTy::Int.wrap(2_147_483_648), -2_147_483_648);
        assert_eq!(IntTy::Byte.wrap(260), 4);
        assert_eq!(IntTy::ULong.value(-1), u64::MAX as i128);
        assert_eq!(IntTy::UInt.value(-1), u32::MAX as i128);
    }

    #[test]
    fn ranges() {
        assert_eq!(IntTy::Int.min(), i32::MIN as i128);
        assert_eq!(IntTy::ULong.max(), u64::MAX as i128);
        assert!(!IntTy::Byte.fits(256));
        assert!(IntTy::Long.fits(i64::MIN as i128));
    }
}
