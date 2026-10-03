//! How a Dream scalar is spelled in C. Shared by the `@c` shim, the `@cpp` bridge, and the
//! analyzer, so every side of the native boundary agrees on widths and signedness.

use crate::PrimTy;
use dream_syntax::nodes::Type;

/// A C scalar type that a Dream primitive maps to one-to-one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CScalar {
    Bool,
    Char,
    U8,
    I32,
    U32,
    I64,
    U64,
    ISize,
    USize,
    F32,
    F64,
}

impl CScalar {
    pub fn of_prim(prim: PrimTy) -> Option<Self> {
        Some(match prim {
            PrimTy::Bool => CScalar::Bool,
            PrimTy::Char => CScalar::Char,
            PrimTy::Byte => CScalar::U8,
            PrimTy::Int => CScalar::I32,
            PrimTy::UInt => CScalar::U32,
            PrimTy::Long => CScalar::I64,
            PrimTy::ULong => CScalar::U64,
            PrimTy::ISize => CScalar::ISize,
            PrimTy::USize => CScalar::USize,
            PrimTy::Float => CScalar::F32,
            PrimTy::Double => CScalar::F64,
            PrimTy::String => return None,
        })
    }

    /// The scalar a source type names, before any type interning.
    pub fn of_type(ty: &Type) -> Option<Self> {
        Some(match ty {
            Type::Boolean(_) => CScalar::Bool,
            Type::Char(_) => CScalar::Char,
            Type::Byte(_) => CScalar::U8,
            Type::Integer(_) => CScalar::I32,
            Type::UInt(_) => CScalar::U32,
            Type::Long(_) => CScalar::I64,
            Type::ULong(_) => CScalar::U64,
            Type::ISize(_) => CScalar::ISize,
            Type::USize(_) => CScalar::USize,
            Type::Float(_) => CScalar::F32,
            Type::Double(_) => CScalar::F64,
            _ => return None,
        })
    }

    pub fn prim(self) -> PrimTy {
        match self {
            CScalar::Bool => PrimTy::Bool,
            CScalar::Char => PrimTy::Char,
            CScalar::U8 => PrimTy::Byte,
            CScalar::I32 => PrimTy::Int,
            CScalar::U32 => PrimTy::UInt,
            CScalar::I64 => PrimTy::Long,
            CScalar::U64 => PrimTy::ULong,
            CScalar::ISize => PrimTy::ISize,
            CScalar::USize => PrimTy::USize,
            CScalar::F32 => PrimTy::Float,
            CScalar::F64 => PrimTy::Double,
        }
    }

    /// The C spelling (`<stdint.h>`/`<stdbool.h>` names, valid in C and C++).
    pub fn c_name(self) -> &'static str {
        match self {
            CScalar::Bool => "bool",
            CScalar::Char => "char",
            CScalar::U8 => "uint8_t",
            CScalar::I32 => "int32_t",
            CScalar::U32 => "uint32_t",
            CScalar::I64 => "int64_t",
            CScalar::U64 => "uint64_t",
            CScalar::ISize => "intptr_t",
            CScalar::USize => "uintptr_t",
            CScalar::F32 => "float",
            CScalar::F64 => "double",
        }
    }

    /// Narrower than the 32-bit register Dream keeps it in: C only defines the low bits, so
    /// values must be re-extended at the boundary rather than passed through.
    pub fn is_narrow(self) -> bool {
        matches!(self, CScalar::Bool | CScalar::Char | CScalar::U8)
    }

    /// The C type of Dream's in-register representation of this scalar.
    pub fn carrier(self) -> &'static str {
        if self.is_narrow() {
            "int32_t"
        } else {
            self.c_name()
        }
    }

    /// C expression converting `e` (of [`Self::c_name`]) to [`Self::carrier`].
    pub fn to_carrier(self, e: &str) -> String {
        match self {
            CScalar::Bool | CScalar::U8 => format!("(int32_t)({e})"),
            CScalar::Char => format!("(int32_t)(unsigned char)({e})"),
            _ => e.to_string(),
        }
    }

    /// C expression converting `e` (of [`Self::carrier`]) to [`Self::c_name`].
    pub fn from_carrier(self, e: &str) -> String {
        match self {
            CScalar::Bool => format!("(({e}) != 0)"),
            CScalar::Char => format!("(char)({e})"),
            CScalar::U8 => format!("(uint8_t)({e})"),
            _ => e.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_scalars_round_trip_through_an_int32_carrier() {
        for s in [CScalar::Bool, CScalar::Char, CScalar::U8] {
            assert!(s.is_narrow());
            assert_eq!(s.carrier(), "int32_t");
            assert_eq!(CScalar::of_prim(s.prim()), Some(s));
        }
        assert_eq!(CScalar::Bool.from_carrier("x"), "((x) != 0)");
        assert_eq!(CScalar::I64.to_carrier("x"), "x");
        assert_eq!(CScalar::of_prim(PrimTy::String), None);
    }
}
