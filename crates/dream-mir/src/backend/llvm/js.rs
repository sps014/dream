//! Calls into JS and `as` casts to and from `js`. Native builds route a call through the host's
//! `dream_js_call`; wasm32 builds call the JS bridge import directly with a tagged argument-slot
//! buffer (layout from [`dream_abi::js_abi`]) and marshal structs through the generated glue.

use super::fx::{Fx, V};
use super::ir::{Ty, Value};
use crate::backend::shared::abi_types::runtime_c_name;
use crate::backend::shared::js_marshal::{box_prim, cast_sym, js_bridge, unbox_prim};
use crate::{Callee, Operand, Place, Rvalue};
use dream_abi::js_abi;
use dream_types::{TyKind, TypeId};

impl<'l, 'a> Fx<'l, 'a> {
    pub fn js_call(
        &mut self,
        callee: &Callee,
        target: &Operand,
        via: &Option<Operand>,
        method: &Option<Operand>,
        args: &[(Operand, TypeId)],
    ) -> Option<V> {
        if !self.l.cx.target.is_wasm32() {
            return Some(self.js_call_native(target, via, method, args.len()));
        }
        let mut host_args = vec![self.operand(target)];
        if let Some(v) = via {
            host_args.push(self.operand(v));
        }
        if let Some(m) = method {
            host_args.push(self.operand(m));
        }
        let slots = if args.is_empty() {
            V::i32(0)
        } else {
            let nbytes = args.len() as u64 * js_abi::SLOT_SIZE as u64;
            let buf = self.alloca_bytes(nbytes, 8);
            self.memset0(&buf, &Value::i64(nbytes as i64));
            for (i, (op, ty)) in args.iter().enumerate() {
                let (tag, aux, store) = js_abi::slot_desc(self.interner, *ty);
                let payload = if tag == js_abi::tag::FUNC {
                    // A funcbox's bare `dream_ft` index; the host maps it to a table slot itself.
                    let fb = self.operand(op);
                    self.call_v("dream_funcbox_funcidx", &[fb])
                } else {
                    self.operand(op)
                };
                let off = i as i64 * js_abi::SLOT_SIZE as i64;
                let at = self.w.gep_const(&buf, off);
                self.store_ty(&Ty::I32, &at, &V::i32(tag as i64), 8);
                let at = self.w.gep_const(&buf, off + js_abi::SLOT_AUX_OFFSET as i64);
                self.store_ty(&Ty::I32, &at, &V::i32(aux as i64), 4);
                let at = self
                    .w
                    .gep_const(&buf, off + js_abi::SLOT_PAYLOAD_OFFSET as i64);
                let t = match store {
                    "i64.store" => Ty::I64,
                    "f64.store" => Ty::F64,
                    "f32.store" => Ty::F32,
                    _ => Ty::I32,
                };
                self.store_ty(&t, &at, &payload, 8);
            }
            self.as_ref(&V::s(buf))
        };
        host_args.push(slots);
        host_args.push(V::i32(args.len() as i64));
        let raw = self.l.cx.callee_sym(callee.def, &callee.args);
        self.call(&runtime_c_name(&raw), &host_args)
    }

    /// A wasm32 cast between `js` and a primitive or struct; `None` for every other cast.
    pub fn js_cast(&mut self, src: &V, from: TypeId, to: TypeId) -> Option<V> {
        if !self.l.cx.target.is_wasm32() {
            return None;
        }
        if let Some(sym) = cast_sym(&self.l.cx, from, to) {
            if matches!(self.interner.kind(from), TyKind::Js) && self.interner.is_value_type(to) {
                crate::internal_error!("js→value-struct cast must be stored in place (got {sym})");
            }
            return Some(self.call_v(&sym, std::slice::from_ref(src)));
        }
        match (
            self.interner.kind(from).clone(),
            self.interner.kind(to).clone(),
        ) {
            (TyKind::Prim(p), TyKind::Js) => {
                let (method, widen) = box_prim(p);
                let v = if widen {
                    V::s(self.conv(src, &Ty::F64))
                } else {
                    src.clone()
                };
                let name = js_bridge(&self.l.cx, method);
                Some(self.call_v(&name, &[v]))
            }
            (TyKind::Js, TyKind::Prim(p)) => {
                let (method, narrow) = unbox_prim(p);
                let name = js_bridge(&self.l.cx, method);
                let v = self.call_v(&name, std::slice::from_ref(src));
                Some(if narrow {
                    V::s(self.conv(&v, &Ty::F32))
                } else {
                    v
                })
            }
            _ => None,
        }
    }

    /// wasm32 `js as ValueStruct`: the marshaler writes straight into the destination.
    pub fn store_js_to_value(&mut self, place: &Place, rv: &Rvalue) -> bool {
        if !self.l.cx.target.is_wasm32() {
            return false;
        }
        let Rvalue::Cast(o, from, to) = rv else {
            return false;
        };
        if !matches!(self.interner.kind(*from), TyKind::Js) || !self.interner.is_value_type(*to) {
            return false;
        }
        let Some(sym) = cast_sym(&self.l.cx, *from, *to) else {
            return false;
        };
        let Some(dst) = self.value_dest(place) else {
            crate::internal_error!("js→value-struct cast has no in-place destination");
        };
        let src = self.operand(o);
        self.call(&sym, &[src, dst]);
        true
    }
}
