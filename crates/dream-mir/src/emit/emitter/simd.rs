//! Inline WASM SIMD for `Vector<T>` `@intrinsic("simd_*")` calls. The callee's instance `T`
//! selects the `v128` lane type; no `$simd_*` runtime helper is invoked.
//!
//! Owning `Vector<T>` locals are WASM `v128` registers. Params/returns still use the sret pointer
//! ABI; [`Self::emit_v128_operand`] loads from memory when the operand is not a `v128` local.

use super::*;
use crate::SimdLane;
use dream_types::TyKind;

impl Emitter<'_> {
    fn simd_lane_of(callee: &crate::Callee, interner: &dream_types::TypeInterner) -> Option<SimdLane> {
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

    fn simd_intrinsic_key(sym: &str) -> String {
        if sym.starts_with("simd_") {
            return sym.to_string();
        }
        let key = if sym.ends_with("load_raw") || sym.ends_with("_load") {
            "simd_v128_load"
        } else if sym.ends_with("store_raw") || sym.ends_with("_store") {
            "simd_v128_store"
        } else if sym.ends_with("splat_raw") || sym.ends_with("_splat") {
            "simd_v128_splat"
        } else if sym.ends_with("bin_add") || sym.ends_with("_add") {
            "simd_v128_add"
        } else if sym.ends_with("bin_sub") || sym.ends_with("_sub") {
            "simd_v128_sub"
        } else if sym.ends_with("bin_mul") || sym.ends_with("_mul") {
            "simd_v128_mul"
        } else if sym.ends_with("bin_min") || sym.ends_with("_min") {
            "simd_v128_min"
        } else if sym.ends_with("bin_max") || sym.ends_with("_max") {
            "simd_v128_max"
        } else if sym.ends_with("reduce_sum") || sym.ends_with("_sum") {
            "simd_v128_sum"
        } else if sym.ends_with("lane_count") || sym.ends_with("_count") {
            "simd_lane_count"
        } else {
            return sym.to_string();
        };
        key.to_string()
    }

    /// Pushes a `v128` value: `local.get` for a register `Vector`, otherwise `v128.load` from an
    /// sret/shadow-frame pointer.
    pub(super) fn emit_v128_operand(&mut self, op: &Operand) {
        if let Operand::Copy(Place::Local(l)) = op {
            if self.is_v128_local(*l) {
                self.line(&format!("     (local.get ${})", l.0));
                return;
            }
        }
        self.emit_operand(op);
        self.line("     (v128.load)");
    }

    fn finish_v128<F: Fn(&mut Self)>(&mut self, dest_v128: Option<u32>, sret: Option<F>) {
        if let Some(d) = dest_v128 {
            self.line(&format!("     (local.set ${d})"));
        } else if sret.is_some() {
            self.line("     (v128.store)");
        }
    }

    /// Emits a `Vector<T>` SIMD intrinsic in place of `call $simd_*`. Returns true when handled.
    /// `dest_v128` is the owning `Vector` local to `local.set`; `sret` pushes a memory destination
    /// address for a following `v128.store`.
    pub(super) fn try_emit_simd_call<F: Fn(&mut Self)>(
        &mut self,
        callee: &crate::Callee,
        args: &[Operand],
        sret: Option<F>,
        dest_v128: Option<u32>,
    ) -> bool {
        let key = Self::simd_intrinsic_key(&self.callee_symbol(callee));
        if key == "simd_lane_count" {
            let lane = Self::simd_lane_of(callee, self.interner).unwrap_or(SimdLane::I32);
            self.line(&format!("     (i32.const {})", lane.count()));
            return true;
        }
        let Some(lane) = Self::simd_lane_of(callee, self.interner).or_else(|| {
            args.iter().find_map(|a| {
                let ty = self.operand_ty(a);
                SimdLane::from_elem(self.interner, ty).or_else(|| match self.interner.kind(ty) {
                    TyKind::Struct(_, ts) => ts.first().and_then(|e| SimdLane::from_elem(self.interner, *e)),
                    TyKind::Array(e) => SimdLane::from_elem(self.interner, *e),
                    _ => None,
                })
            })
        }) else {
            return false;
        };
        match key.as_str() {
            "simd_v128_splat" => {
                if let Some(push) = sret.as_ref() {
                    push(self);
                } else if dest_v128.is_none() {
                    return false;
                }
                if let Some(v) = args.first() {
                    self.emit_operand(v);
                }
                self.line(&format!("     ({})", lane.splat_wat()));
                self.finish_v128(dest_v128, sret);
                true
            }
            "simd_v128_load" => {
                if let Some(push) = sret.as_ref() {
                    push(self);
                } else if dest_v128.is_none() {
                    return false;
                }
                if args.len() >= 2 {
                    self.emit_v128_array_addr(&args[0], &args[1], lane);
                }
                self.line("     (v128.load)");
                self.finish_v128(dest_v128, sret);
                true
            }
            "simd_v128_store" => {
                if args.len() >= 3 {
                    self.emit_v128_array_addr(&args[1], &args[2], lane);
                    self.emit_v128_operand(&args[0]);
                    self.line("     (v128.store)");
                }
                true
            }
            "simd_v128_add" | "simd_v128_sub" | "simd_v128_mul" | "simd_v128_min"
            | "simd_v128_max" => {
                if let Some(push) = sret.as_ref() {
                    push(self);
                } else if dest_v128.is_none() {
                    return false;
                }
                let op = match key.as_str() {
                    "simd_v128_add" => lane.binop_wat(BinOp::Add).unwrap_or("i32x4.add"),
                    "simd_v128_sub" => lane.binop_wat(BinOp::Sub).unwrap_or("i32x4.sub"),
                    "simd_v128_mul" => lane.binop_wat(BinOp::Mul).unwrap_or("i32x4.mul"),
                    "simd_v128_min" => lane.min_wat(),
                    _ => lane.max_wat(),
                };
                if args.len() >= 2 {
                    self.emit_v128_operand(&args[0]);
                    self.emit_v128_operand(&args[1]);
                }
                self.line(&format!("     ({op})"));
                self.finish_v128(dest_v128, sret);
                true
            }
            "simd_v128_sum" => {
                if let Some(v) = args.first() {
                    self.emit_v128_operand(v);
                    self.emit_v128_sum(lane);
                }
                true
            }
            _ => false,
        }
    }

    fn emit_v128_sum(&mut self, lane: SimdLane) {
        match lane {
            SimdLane::F32 => {
                self.line("     (local.set $__v128)");
                self.line("     (local.get $__v128)");
                self.line("     (f32x4.extract_lane 0)");
                self.line("     (local.get $__v128)");
                self.line("     (f32x4.extract_lane 1)");
                self.line("     (f32.add)");
                self.line("     (local.get $__v128)");
                self.line("     (f32x4.extract_lane 2)");
                self.line("     (f32.add)");
                self.line("     (local.get $__v128)");
                self.line("     (f32x4.extract_lane 3)");
                self.line("     (f32.add)");
            }
            SimdLane::I32 => {
                self.line("     (local.set $__v128)");
                self.line("     (local.get $__v128)");
                self.line("     (i32x4.extract_lane 0)");
                self.line("     (local.get $__v128)");
                self.line("     (i32x4.extract_lane 1)");
                self.line("     (i32.add)");
                self.line("     (local.get $__v128)");
                self.line("     (i32x4.extract_lane 2)");
                self.line("     (i32.add)");
                self.line("     (local.get $__v128)");
                self.line("     (i32x4.extract_lane 3)");
                self.line("     (i32.add)");
            }
            SimdLane::F64 => {
                self.line("     (local.set $__v128)");
                self.line("     (local.get $__v128)");
                self.line("     (f64x2.extract_lane 0)");
                self.line("     (local.get $__v128)");
                self.line("     (f64x2.extract_lane 1)");
                self.line("     (f64.add)");
            }
            SimdLane::I64 => {
                self.line("     (local.set $__v128)");
                self.line("     (local.get $__v128)");
                self.line("     (i64x2.extract_lane 0)");
                self.line("     (local.get $__v128)");
                self.line("     (i64x2.extract_lane 1)");
                self.line("     (i64.add)");
            }
            SimdLane::I8 => {
                self.line("     (local.set $__v128)");
                self.line("     (i32.const 0)");
                self.line("     (local.set $__len)");
                for i in 0..16 {
                    self.line("     (local.get $__v128)");
                    self.line(&format!("     (i8x16.extract_lane_s {})", i));
                    self.line("     (local.get $__len)");
                    self.line("     (i32.add)");
                    self.line("     (local.set $__len)");
                }
                self.line("     (local.get $__len)");
            }
        }
    }
}
