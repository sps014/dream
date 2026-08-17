//! WASM `v128` lane kinds shared by autovec and `Vector<T>` emission.

use dream_types::{PrimTy, TyKind, TypeId, TypeInterner};

use crate::BinOp;

/// 16-byte SIMD lane layout selected by the array / `Vector<T>` element type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdLane {
    I8,
    I32,
    I64,
    F32,
    F64,
}

impl SimdLane {
    pub fn from_elem(interner: &TypeInterner, ty: TypeId) -> Option<Self> {
        match interner.kind(ty) {
            TyKind::Prim(PrimTy::Byte) => Some(SimdLane::I8),
            TyKind::Prim(PrimTy::Int) => Some(SimdLane::I32),
            TyKind::Prim(PrimTy::Long) => Some(SimdLane::I64),
            TyKind::Prim(PrimTy::Float) => Some(SimdLane::F32),
            TyKind::Prim(PrimTy::Double) => Some(SimdLane::F64),
            _ => None,
        }
    }

    pub fn count(self) -> i64 {
        match self {
            SimdLane::I8 => 16,
            SimdLane::I32 | SimdLane::F32 => 4,
            SimdLane::I64 | SimdLane::F64 => 2,
        }
    }

    pub fn esize(self) -> u32 {
        match self {
            SimdLane::I8 => 1,
            SimdLane::I32 | SimdLane::F32 => 4,
            SimdLane::I64 | SimdLane::F64 => 8,
        }
    }

    pub fn shift(self) -> u32 {
        match self {
            SimdLane::I8 => 0,
            SimdLane::I32 | SimdLane::F32 => 2,
            SimdLane::I64 | SimdLane::F64 => 3,
        }
    }

    pub fn supports_binop(self, op: BinOp) -> bool {
        matches!(
            (self, op),
            (
                SimdLane::F32 | SimdLane::I32 | SimdLane::I64 | SimdLane::F64,
                BinOp::Add | BinOp::Sub | BinOp::Mul
            ) | (SimdLane::I8, BinOp::Add | BinOp::Sub)
        )
    }
}
