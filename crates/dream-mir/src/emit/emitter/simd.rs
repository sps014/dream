//! Inline WASM SIMD for `Vector<T>` `@intrinsic("simd_*")` calls. The callee's instance `T`
//! selects the `v128` lane type; no `$simd_*` runtime helper is invoked.
//!
//! Owning `Vector<T>` locals are WASM `v128` registers. Params/returns still use the sret pointer
//! ABI; [`Self::emit_v128_operand`] loads from memory when the operand is not a `v128` local.

use super::*;
use crate::SimdLane;
use dream_abi::intrinsics::IntrinsicOp;
use dream_types::TyKind;

impl Emitter<'_> {
    fn simd_lane_of(
        callee: &crate::Callee,
        interner: &dream_types::TypeInterner,
    ) -> Option<SimdLane> {
        let mut consider = Vec::new();
        consider.extend(callee.args.iter().copied());
        consider.push(callee.ret);
        for t in consider {
            if let Some(l) = SimdLane::from_elem(interner, t) {
                return Some(l);
            }
            match interner.kind(t) {
                TyKind::Struct(_, args) => {
                    if let Some(&e) = args.first() {
                        if let Some(l) = SimdLane::from_elem(interner, e) {
                            return Some(l);
                        }
                    }
                }
                TyKind::Array(e) => {
                    if let Some(l) = SimdLane::from_elem(interner, *e) {
                        return Some(l);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn simd_binop(lane: SimdLane, op: BinOp) -> Nullary {
        match (lane, op) {
            (SimdLane::F32, BinOp::Add) => Nullary::F32x4Add,
            (SimdLane::F32, BinOp::Sub) => Nullary::F32x4Sub,
            (SimdLane::F32, BinOp::Mul) => Nullary::F32x4Mul,
            (SimdLane::I32, BinOp::Add) => Nullary::I32x4Add,
            (SimdLane::I32, BinOp::Sub) => Nullary::I32x4Sub,
            (SimdLane::I32, BinOp::Mul) => Nullary::I32x4Mul,
            (SimdLane::I64, BinOp::Add) => Nullary::I64x2Add,
            (SimdLane::I64, BinOp::Sub) => Nullary::I64x2Sub,
            (SimdLane::I64, BinOp::Mul) => Nullary::I64x2Mul,
            (SimdLane::F64, BinOp::Add) => Nullary::F64x2Add,
            (SimdLane::F64, BinOp::Sub) => Nullary::F64x2Sub,
            (SimdLane::F64, BinOp::Mul) => Nullary::F64x2Mul,
            (SimdLane::I8, BinOp::Add) => Nullary::I8x16Add,
            (SimdLane::I8, BinOp::Sub) => Nullary::I8x16Sub,
            _ => crate::internal_error!("unsupported SIMD binop {op:?} for {lane:?}"),
        }
    }

    fn simd_min(lane: SimdLane) -> Nullary {
        match lane {
            SimdLane::F32 => Nullary::F32x4Min,
            SimdLane::F64 => Nullary::F64x2Min,
            SimdLane::I32 => Nullary::I32x4MinS,
            SimdLane::I8 => Nullary::I8x16MinS,
            SimdLane::I64 => crate::internal_error!("WASM SIMD has no i64x2.min"),
        }
    }

    fn simd_max(lane: SimdLane) -> Nullary {
        match lane {
            SimdLane::F32 => Nullary::F32x4Max,
            SimdLane::F64 => Nullary::F64x2Max,
            SimdLane::I32 => Nullary::I32x4MaxS,
            SimdLane::I8 => Nullary::I8x16MaxS,
            SimdLane::I64 => crate::internal_error!("WASM SIMD has no i64x2.max"),
        }
    }

    fn simd_splat(lane: SimdLane) -> Nullary {
        match lane {
            SimdLane::F32 => Nullary::F32x4Splat,
            SimdLane::F64 => Nullary::F64x2Splat,
            SimdLane::I32 => Nullary::I32x4Splat,
            SimdLane::I64 => Nullary::I64x2Splat,
            SimdLane::I8 => Nullary::I8x16Splat,
        }
    }

    fn simd_extract(lane: SimdLane) -> ExtractLane {
        match lane {
            SimdLane::F32 => ExtractLane::F32x4,
            SimdLane::I32 => ExtractLane::I32x4,
            SimdLane::F64 => ExtractLane::F64x2,
            SimdLane::I64 => ExtractLane::I64x2,
            SimdLane::I8 => ExtractLane::I8x16S,
        }
    }

    /// Pushes a `v128` value: `local.get` for a register `Vector`, otherwise `v128.load` from an
    /// sret/shadow-frame pointer.
    pub(super) fn emit_v128_operand(&mut self, op: &Operand) {
        if let Operand::Copy(Place::Local(l)) = op {
            if self.is_v128_local(*l) {
                self.f.local_get(&(l.0).to_string());
                return;
            }
        }
        self.emit_operand(op);
        self.f.load(LoadKind::V128, 0);
    }

    fn finish_v128<F: Fn(&mut Self)>(&mut self, dest_v128: Option<u32>, sret: Option<F>) {
        if let Some(d) = dest_v128 {
            self.f.local_set(&d.to_string());
        } else if sret.is_some() {
            self.f.store(StoreKind::V128, 0);
        }
    }

    /// Emits a `Vector<T>` SIMD intrinsic in place of `call $simd_*`. Returns true when handled.
    pub(super) fn try_emit_simd_call<F: Fn(&mut Self)>(
        &mut self,
        callee: &crate::Callee,
        args: &[Operand],
        sret: Option<F>,
        dest_v128: Option<u32>,
    ) -> bool {
        let Some(key) = self
            .intrinsics
            .get(&callee.def)
            .copied()
            .or_else(|| IntrinsicOp::from_key(&self.callee_symbol(callee)))
        else {
            return false;
        };
        if !key.is_simd() {
            return false;
        }
        if key == IntrinsicOp::SimdLaneCount {
            let lane = Self::simd_lane_of(callee, self.interner).unwrap_or(SimdLane::I32);
            self.f.i32_const(lane.count() as i32);
            return true;
        }
        let Some(lane) = Self::simd_lane_of(callee, self.interner).or_else(|| {
            args.iter().find_map(|a| {
                let ty = self.operand_ty(a);
                SimdLane::from_elem(self.interner, ty).or_else(|| match self.interner.kind(ty) {
                    TyKind::Struct(_, ts) => ts
                        .first()
                        .and_then(|e| SimdLane::from_elem(self.interner, *e)),
                    TyKind::Array(e) => SimdLane::from_elem(self.interner, *e),
                    _ => None,
                })
            })
        }) else {
            return false;
        };
        match key {
            IntrinsicOp::SimdV128Splat => {
                if let Some(push) = sret.as_ref() {
                    push(self);
                } else if dest_v128.is_none() {
                    return false;
                }
                if let Some(v) = args.first() {
                    self.emit_operand(v);
                }
                self.f.nullary(Self::simd_splat(lane));
                self.finish_v128(dest_v128, sret);
                true
            }
            IntrinsicOp::SimdV128Load => {
                if let Some(push) = sret.as_ref() {
                    push(self);
                } else if dest_v128.is_none() {
                    return false;
                }
                if args.len() >= 2 {
                    self.emit_v128_array_addr(&args[0], &args[1], lane);
                }
                self.f.load(LoadKind::V128, 0);
                self.finish_v128(dest_v128, sret);
                true
            }
            IntrinsicOp::SimdV128Store => {
                if args.len() >= 3 {
                    self.emit_v128_array_addr(&args[1], &args[2], lane);
                    self.emit_v128_operand(&args[0]);
                    self.f.store(StoreKind::V128, 0);
                }
                true
            }
            IntrinsicOp::SimdV128Add
            | IntrinsicOp::SimdV128Sub
            | IntrinsicOp::SimdV128Mul
            | IntrinsicOp::SimdV128Min
            | IntrinsicOp::SimdV128Max => {
                if let Some(push) = sret.as_ref() {
                    push(self);
                } else if dest_v128.is_none() {
                    return false;
                }
                let op = match key {
                    IntrinsicOp::SimdV128Add => Self::simd_binop(lane, BinOp::Add),
                    IntrinsicOp::SimdV128Sub => Self::simd_binop(lane, BinOp::Sub),
                    IntrinsicOp::SimdV128Mul => Self::simd_binop(lane, BinOp::Mul),
                    IntrinsicOp::SimdV128Min => Self::simd_min(lane),
                    _ => Self::simd_max(lane),
                };
                if args.len() >= 2 {
                    self.emit_v128_operand(&args[0]);
                    self.emit_v128_operand(&args[1]);
                }
                self.f.nullary(op);
                self.finish_v128(dest_v128, sret);
                true
            }
            IntrinsicOp::SimdV128Sum => {
                if let Some(v) = args.first() {
                    self.emit_v128_operand(v);
                    self.emit_v128_sum(lane);
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn emit_simd_splat(&mut self, lane: SimdLane) {
        self.f.nullary(Self::simd_splat(lane));
    }

    pub(super) fn emit_simd_binop(&mut self, lane: SimdLane, op: BinOp) {
        self.f.nullary(Self::simd_binop(lane, op));
    }

    fn emit_v128_sum(&mut self, lane: SimdLane) {
        let extract = Self::simd_extract(lane);
        let n = lane.count() as u8;
        self.f.local_set("__v128");
        match lane {
            SimdLane::I8 => {
                self.f.i32_const(0);
                self.f.local_set("__len");
                for i in 0..n {
                    self.f.local_get("__v128");
                    self.f.extract_lane(extract, i);
                    self.f.local_get("__len");
                    self.f.i32_add();
                    self.f.local_set("__len");
                }
                self.f.local_get("__len");
            }
            _ => {
                self.f.local_get("__v128");
                self.f.extract_lane(extract, 0);
                for i in 1..n {
                    self.f.local_get("__v128");
                    self.f.extract_lane(extract, i);
                    match lane {
                        SimdLane::F32 => self.f.f32_add(),
                        SimdLane::I32 => self.f.i32_add(),
                        SimdLane::F64 => self.f.f64_add(),
                        SimdLane::I64 => self.f.i64_add(),
                        SimdLane::I8 => unreachable!(),
                    }
                }
            }
        }
    }
}
