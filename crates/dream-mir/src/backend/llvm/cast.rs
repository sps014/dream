//! `Rvalue::Cast`: primitive conversions, boxing to `object`, and checked unboxing.

use super::fx::{Fx, V};
use super::ir::{Ty, Value};
use crate::Operand;
use crate::backend::shared::abi_types::{elem_size, mem_ty};
use crate::backend::shared::protocol_names::runtime_tag;
use dream_types::{PrimTy, TyKind, TypeId};

/// The numeric meaning of a scalar cast operand. Every integer type narrower than 32 bits lives
/// in an `i32` register, normalized to its own range.
#[derive(Clone, Copy)]
enum Scalar {
    Int { bits: u32, unsigned: bool },
    Float,
}

impl<'l, 'a> Fx<'l, 'a> {
    pub fn emit_cast(&mut self, v: &Operand, from: TypeId, to: TypeId) -> V {
        let src = self.operand(v);
        if from == to {
            return src;
        }
        if let Some(r) = self.js_cast(&src, from, to) {
            return r;
        }
        let fk = self.interner.kind(from).clone();
        let tk = self.interner.kind(to).clone();
        if let (Some(source), Some(target)) = (self.scalar(&fk), self.scalar(&tk)) {
            return self.scalar_cast(&src, from, to, source, target);
        }
        if matches!(tk, TyKind::Object | TyKind::Interface(..)) && self.is_value(from) {
            let size = elem_size(&self.l.cx, from) as i64;
            let tag = self.l.cx.type_tag(from);
            let b = self.call_v("dream_malloc", &[V::i64(size), V::i32(tag as i64)]);
            let (bp, sp) = (self.ptr(&b), self.ptr(&src));
            self.memcpy(&bp, &sp, &Value::i64(size));
            return b;
        }
        use PrimTy as P;
        match (&fk, &tk) {
            (_, TyKind::Object) => {
                if matches!(
                    fk,
                    TyKind::Enum(_)
                        | TyKind::Prim(
                            P::ISize | P::USize | P::Float16 | P::SByte | P::Short | P::UShort
                        )
                ) {
                    let size = elem_size(&self.l.cx, from) as i64;
                    let tag = runtime_tag(&self.l.cx, from);
                    let boxed = self.call_v("dream_malloc", &[V::i64(size), V::i32(tag as i64)]);
                    let address = self.ptr(&boxed);
                    self.store_mem(mem_ty(&self.l.cx, from), &address, &src, size as u32);
                    return boxed;
                }
                let (f, t) = match fk {
                    TyKind::Prim(P::Int) => ("dream_box_int", Ty::I32),
                    TyKind::Prim(P::Float) => ("dream_box_float", Ty::F32),
                    TyKind::Prim(P::Double) => ("dream_box_double", Ty::F64),
                    TyKind::Prim(P::Bool) => ("dream_box_bool", Ty::I32),
                    TyKind::Prim(P::Char) => ("dream_box_char", Ty::I32),
                    TyKind::Prim(P::Long) => ("dream_box_long", Ty::I64),
                    TyKind::Prim(P::UInt) => ("dream_box_uint", Ty::I32),
                    TyKind::Prim(P::ULong) => ("dream_box_ulong", Ty::I64),
                    TyKind::Prim(P::Byte) => ("dream_box_byte", Ty::I32),
                    _ => return src,
                };
                let x = self.conv_v(&src, &t, false);
                self.call_v(f, &[x])
            }
            (TyKind::Object | TyKind::Interface(..), TyKind::Prim(P::Float16)) => {
                let boxed = self.check_box_tag(&src, to);
                let address = self.ptr(&boxed);
                self.load_ty(Ty::F16, &address, 2, false)
            }
            (TyKind::Object | TyKind::Interface(..), TyKind::Prim(p)) => {
                let unbox = match p {
                    P::Int => "dream_unbox_int",
                    P::Float => "dream_unbox_float",
                    P::Double => "dream_unbox_double",
                    P::Bool => "dream_unbox_bool",
                    P::Char => "dream_unbox_char",
                    P::Long => "dream_unbox_long",
                    P::UInt => "dream_unbox_uint",
                    P::ULong => "dream_unbox_ulong",
                    P::ISize if self.l.cx.mir.layouts.target.ptr_size == 8 => "dream_unbox_long",
                    P::USize if self.l.cx.mir.layouts.target.ptr_size == 8 => "dream_unbox_ulong",
                    P::ISize => "dream_unbox_int",
                    P::USize => "dream_unbox_uint",
                    P::Byte => "dream_unbox_byte",
                    P::SByte | P::Short | P::UShort => {
                        let boxed = self.check_box_tag(&src, to);
                        let address = self.ptr(&boxed);
                        let (ty, align, unsigned) = match p {
                            P::SByte => (Ty::I8, 1, false),
                            P::Short => (Ty::I16, 2, false),
                            _ => (Ty::I16, 2, true),
                        };
                        return self.load_ty(ty, &address, align, unsigned);
                    }
                    P::String => return src,
                    P::Float16 => unreachable!("float16 unboxes inline above"),
                };
                self.checked_unbox(&src, to, unbox)
            }
            (TyKind::Object | TyKind::Interface(..), TyKind::Enum(_)) => {
                self.call_v("dream_unbox_int", &[src])
            }
            // Lowering passes a closure environment reference as an `int`-typed handle; the source
            // language rejects these casts. The handle keeps its full pointer width.
            (_, TyKind::Prim(_)) | (TyKind::Prim(_), _)
                if matches!(
                    (self.scalar(&fk), self.scalar(&tk)),
                    (None, Some(Scalar::Int { .. })) | (Some(Scalar::Int { .. }), None)
                ) && !self.is_value(from)
                    && !self.is_value(to) =>
            {
                src
            }
            _ if self.scalar(&fk).is_some() || self.scalar(&tk).is_some() => {
                crate::internal_error!("no cast lowering from {fk:?} to {tk:?}")
            }
            // Reference reinterpretations (upcasts, checked downcasts, `string` to `object`) keep
            // the same pointer.
            _ => src,
        }
    }

    fn scalar(&self, kind: &TyKind) -> Option<Scalar> {
        use PrimTy as P;
        let int = |bits, unsigned| Some(Scalar::Int { bits, unsigned });
        match kind {
            TyKind::Prim(P::SByte) => int(8, false),
            TyKind::Prim(P::Byte) => int(8, true),
            TyKind::Prim(P::Short) => int(16, false),
            TyKind::Prim(P::UShort) => int(16, true),
            TyKind::Prim(P::Int) | TyKind::Enum(_) => int(32, false),
            TyKind::Prim(P::UInt | P::Char) => int(32, true),
            TyKind::Prim(P::Long) => int(64, false),
            TyKind::Prim(P::ULong) => int(64, true),
            TyKind::Prim(P::ISize | P::USize) => {
                let Ty::Int(bits) = self.word() else {
                    crate::internal_error!("target word is not an integer type")
                };
                int(bits, matches!(kind, TyKind::Prim(P::USize)))
            }
            TyKind::Prim(P::Float16 | P::Float | P::Double) => Some(Scalar::Float),
            _ => None,
        }
    }

    /// Integer conversions wrap to the target width; float-to-integer conversions saturate at the
    /// target range.
    fn scalar_cast(
        &mut self,
        src: &V,
        from: TypeId,
        to: TypeId,
        source: Scalar,
        target: Scalar,
    ) -> V {
        let from_reg = super::types::ll_ty(self.interner, from, &self.h(), &self.word());
        let to_reg = super::types::ll_ty(self.interner, to, &self.h(), &self.word());
        match (source, target) {
            (Scalar::Int { unsigned: fu, .. }, Scalar::Int { bits, unsigned }) => {
                let x = self.conv_v(src, &from_reg, fu);
                let wrapped = V {
                    v: self.conv(&x, &Ty::Int(bits)),
                    unsigned,
                };
                self.conv_v(&wrapped, &to_reg, unsigned)
            }
            (Scalar::Int { unsigned, .. }, Scalar::Float) => {
                let x = self.conv_v(src, &from_reg, unsigned);
                self.conv_v(&x, &to_reg, false)
            }
            (Scalar::Float, Scalar::Int { bits, unsigned }) => {
                let x = self.conv_v(src, &from_reg, false);
                let saturated = V {
                    v: self.float_to_int_sat(&x, bits, unsigned),
                    unsigned,
                };
                self.conv_v(&saturated, &to_reg, unsigned)
            }
            (Scalar::Float, Scalar::Float) => {
                let x = self.conv_v(src, &from_reg, false);
                self.conv_v(&x, &to_reg, false)
            }
        }
    }

    fn checked_unbox(&mut self, src: &V, ty: TypeId, unbox: &str) -> V {
        let boxed = self.check_box_tag(src, ty);
        self.call_v(unbox, &[boxed])
    }

    /// Traps unless boxed `src` carries `ty`'s runtime tag; returns it as a reference.
    fn check_box_tag(&mut self, src: &V, ty: TypeId) -> V {
        let tag = runtime_tag(&self.l.cx, ty);
        let boxed = self.as_ref(src);
        let t = self.call_v("dream_object_kind", std::slice::from_ref(&boxed));
        let t = self.conv(&t, &Ty::I32);
        let bad = self.w.icmp("ne", &t, &Value::i32(tag as i64));
        self.if_then(&bad, |fx| {
            fx.panic_with(crate::backend::shared::panic_msgs::INVALID_CAST)
        });
        boxed
    }
}
