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

    pub fn binop_wat(self, op: BinOp) -> Option<&'static str> {
        Some(match (self, op) {
            (SimdLane::F32, BinOp::Add) => "f32x4.add",
            (SimdLane::F32, BinOp::Sub) => "f32x4.sub",
            (SimdLane::F32, BinOp::Mul) => "f32x4.mul",
            (SimdLane::I32, BinOp::Add) => "i32x4.add",
            (SimdLane::I32, BinOp::Sub) => "i32x4.sub",
            (SimdLane::I32, BinOp::Mul) => "i32x4.mul",
            (SimdLane::I64, BinOp::Add) => "i64x2.add",
            (SimdLane::I64, BinOp::Sub) => "i64x2.sub",
            (SimdLane::I64, BinOp::Mul) => "i64x2.mul",
            (SimdLane::F64, BinOp::Add) => "f64x2.add",
            (SimdLane::F64, BinOp::Sub) => "f64x2.sub",
            (SimdLane::F64, BinOp::Mul) => "f64x2.mul",
            (SimdLane::I8, BinOp::Add) => "i8x16.add",
            (SimdLane::I8, BinOp::Sub) => "i8x16.sub",
            (SimdLane::I8, BinOp::Mul) => "i8x16.mul",
            _ => return None,
        })
    }

    pub fn min_wat(self) -> &'static str {
        match self {
            SimdLane::F32 => "f32x4.min",
            SimdLane::F64 => "f64x2.min",
            SimdLane::I32 => "i32x4.min_s",
            SimdLane::I64 => "i64x2.min_s",
            SimdLane::I8 => "i8x16.min_s",
        }
    }

    pub fn max_wat(self) -> &'static str {
        match self {
            SimdLane::F32 => "f32x4.max",
            SimdLane::F64 => "f64x2.max",
            SimdLane::I32 => "i32x4.max_s",
            SimdLane::I64 => "i64x2.max_s",
            SimdLane::I8 => "i8x16.max_s",
        }
    }

    pub fn splat_wat(self) -> &'static str {
        match self {
            SimdLane::F32 => "f32x4.splat",
            SimdLane::F64 => "f64x2.splat",
            SimdLane::I32 => "i32x4.splat",
            SimdLane::I64 => "i64x2.splat",
            SimdLane::I8 => "i8x16.splat",
        }
    }
}
