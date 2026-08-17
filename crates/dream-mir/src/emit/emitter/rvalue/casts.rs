//! Cast emission and the numeric-conversion helpers it shares with call-argument widening: value
//! struct boxing, primitive box/unbox, struct<->js marshaling dispatch, and the WASM numeric
//! conversion instruction selection. Methods on the parent module's private `Emitter`.

use super::*;

impl Emitter<'_> {
    /// Boxes a value struct `ty` (whose operand pushes its inline address) into a fresh tagged heap
    /// object: `$malloc(size, tag)`, `memory.copy` the inline bytes in, then retain the copy's
    /// embedded references. Leaves the heap data pointer on the stack (refcount 1, owned).
    fn emit_box_value_struct(&mut self, o: &Operand, ty: TypeId) {
        let size = self.value_size(ty);
        let tag = self.type_tag(ty, dream_types::DefId(0));
        self.f.i32_const((size) as i32);
        self.f.i32_const(tag);
        self.f.call("malloc");
        self.f.local_set("__obj");
        // memory.copy(dst = $__obj, src = inline address of the value, size)
        self.f.local_get("__obj");
        self.emit_operand(o);
        self.f.i32_const((size) as i32);
        self.f.memory_copy();
        if self.value_has_glue(ty) {
            if let Some(name) = self.value_name(ty) {
                self.f.local_get("__obj");
                self.f.call(&vs_retain_sym(&name));
            }
        }
        self.f.local_get("__obj");
    }

    pub(super) fn emit_cast(&mut self, o: &Operand, from: TypeId, to: TypeId) {
        // A struct/class <-> `js` cast routes through the generated deep-copy marshalers (see
        // `js_marshal`); everything else falls through to the primitive box/unbox path below.
        // Value-struct reconstruction is in-place (`$js_to_Name(j, dst)`) and only valid via
        // [`emit_value_store`](Self::emit_value_store) — never as a stack-producing call.
        if let Some(sym) = js_marshal::cast_sym(self.interner, self.layouts, from, to) {
            if matches!(self.interner.kind(from), TyKind::Js) && self.interner.is_value_type(to) {
                crate::internal_error!(
                    "js→value-struct cast must be stored in place (got stack emit of {})",
                    sym
                );
            }
            self.emit_operand(o);
            self.f.call(&sym);
            return;
        }
        // Boxing a value struct into a reference target (`object` or an interface): allocate a
        // tagged heap block, byte-copy the inline value in, and retain the copy's embedded
        // references. The result is a refcounted heap object indistinguishable from a class
        // instance for dynamic dispatch, the object protocol, and deep release.
        let to_is_ref_box = matches!(
            self.interner.kind(to),
            TyKind::Object | TyKind::Interface(..)
        );
        if to_is_ref_box && self.interner.is_value_type(from) {
            self.emit_box_value_struct(o, from);
            return;
        }
        let from_prim = prim_of(self.interner, from);
        let to_prim = prim_of(self.interner, to);
        let to_is_object = matches!(self.interner.kind(to), TyKind::Object);
        let from_is_object = matches!(self.interner.kind(from), TyKind::Object);
        // Boxing a primitive into `object` (reference types are already pointers → identity).
        if to_is_object {
            self.emit_operand(o);
            if let Some(boxfn) = from_prim.and_then(box_fn_for) {
                self.f.call(boxfn);
            }
            return;
        }
        // Unboxing `object` to a primitive (or leaving a reference pointer as-is). An unbox to the
        // wrong primitive checks the boxed value's runtime tag first and panics on mismatch, instead
        // of silently reinterpreting the wrong bytes (e.g. reading a boxed `string`'s data pointer as
        // an `int`).
        if from_is_object {
            if let Some(unboxfn) = to_prim.and_then(unbox_fn_for) {
                if let Some(expected_tag) = runtime_tag_for(self.interner, self.tags, to) {
                    self.emit_operand(o);
                    self.f.call("object_tag");
                    self.f.i32_const(expected_tag);
                    self.f.i32_ne();
                    self.f.if_();
                    self.emit_panic(super::super::super::panic_msgs::INVALID_CAST);
                    self.f.end();
                }
                self.emit_operand(o);
                self.f.call(unboxfn);
            } else {
                self.emit_operand(o);
            }
            return;
        }
        self.emit_operand(o);
        self.emit_numeric_conv(from, to);
        // Narrowing to `byte` (which shares the `i32` WASM type with `int`/`uint`, so `numeric_conv`
        // is a no-op) must wrap into the [0, 255] range explicitly (C-style truncation).
        if matches!(to_prim, Some(PrimTy::Byte)) {
            self.f.i32_const(255);
            self.f.i32_and();
        }
    }

    /// The common numeric type of a binary operation's operands: the one with the wider WASM value
    /// type, so the narrower side can be widened up to it. Ranking `i32 < i64 < f32 < f64` matches the
    /// language's implicit numeric widening (e.g. `long` op `int` -> `long`; any op `double` ->
    /// `double`). Non-numeric operands (equal-width pointers, `bool`, refs) fall through to `a`, which
    /// leaves same-width pairs unchanged (`emit_numeric_conv` is then a no-op).
    pub(super) fn wider_numeric(&self, a: TypeId, b: TypeId) -> TypeId {
        let rank = |w: &str| match w {
            "i32" => 0,
            "i64" => 1,
            "f32" => 2,
            "f64" => 3,
            _ => -1,
        };
        if rank(self.wasm_ty(b).as_str()) > rank(self.wasm_ty(a).as_str()) {
            b
        } else {
            a
        }
    }

    /// Emits the WASM numeric conversion instruction to turn a value of type `from` (already on the
    /// stack) into type `to`, if their WASM value types differ (a no-op otherwise). Shared by explicit
    /// `Cast` and the implicit widening applied to call arguments.
    pub(super) fn emit_numeric_conv(&mut self, from: TypeId, to: TypeId) {
        let (fw, tw) = (
            wasm_val_ty(self.interner, from),
            wasm_val_ty(self.interner, to),
        );
        if fw == tw {
            return;
        }
        let int_signed = |ty: TypeId| {
            !matches!(
                self.interner.kind(ty),
                TyKind::Prim(PrimTy::UInt | PrimTy::ULong | PrimTy::Byte)
            )
        };
        match (fw, tw) {
            (ValType::I32, ValType::I64) => {
                if int_signed(from) {
                    self.f.i64_extend_i32_s();
                } else {
                    self.f.i64_extend_i32_u();
                }
            }
            (ValType::I64, ValType::I32) => self.f.i32_wrap_i64(),
            (ValType::I32, ValType::F32) => {
                if int_signed(from) {
                    self.f.f32_convert_i32_s();
                } else {
                    self.f.f32_convert_i32_u();
                }
            }
            (ValType::I32, ValType::F64) => {
                if int_signed(from) {
                    self.f.f64_convert_i32_s();
                } else {
                    self.f.f64_convert_i32_u();
                }
            }
            (ValType::I64, ValType::F32) => {
                if int_signed(from) {
                    self.f.f32_convert_i64_s();
                } else {
                    self.f.f32_convert_i64_u();
                }
            }
            (ValType::I64, ValType::F64) => {
                if int_signed(from) {
                    self.f.f64_convert_i64_s();
                } else {
                    self.f.f64_convert_i64_u();
                }
            }
            (ValType::F32, ValType::F64) => self.f.f64_promote_f32(),
            (ValType::F64, ValType::F32) => self.f.f32_demote_f64(),
            (ValType::F32, ValType::I32) => {
                if int_signed(to) {
                    self.f.i32_trunc_sat_f32_s();
                } else {
                    self.f.i32_trunc_sat_f32_u();
                }
            }
            (ValType::F64, ValType::I32) => {
                if int_signed(to) {
                    self.f.i32_trunc_sat_f64_s();
                } else {
                    self.f.i32_trunc_sat_f64_u();
                }
            }
            (ValType::F32, ValType::I64) => {
                if int_signed(to) {
                    self.f.i64_trunc_sat_f32_s();
                } else {
                    self.f.i64_trunc_sat_f32_u();
                }
            }
            (ValType::F64, ValType::I64) => {
                if int_signed(to) {
                    self.f.i64_trunc_sat_f64_s();
                } else {
                    self.f.i64_trunc_sat_f64_u();
                }
            }
            _ => self.f.nop(),
        }
    }
}
