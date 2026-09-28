//! wasm32 struct/union/array `<-> js` marshalers (policy in `shared::js_marshal`). Every JS value
//! is a `dream_ptr` handle the host's `js.*` bridges create and read.

use super::super::fx::{align_at, Fx, V};
use super::super::ir::{Ty, Value};
use super::super::lcx::Lcx;
use super::{glue, register};
use crate::abi::LEN_PREFIX_SIZE;
use crate::backend::shared::abi_types::{elem_size, mem_ty, native_scalar_size};
use crate::backend::shared::js_marshal::{
    array_elems, array_syms, box_prim, emits_js_marshal, is_marshalable, is_option_union,
    js_bridge, struct_syms, unbox_prim,
};
use dream_hir::{TypeLayout, UnionLayout, UnionVariant};
use dream_types::{TyKind, TypeId};

/// Marshaled addresses may be interior (a value field of another struct), so only the
/// allocator's 4-byte minimum is assumed.
const MARSHAL_ALIGN: u32 = 4;

fn marshaled_structs(l: &Lcx<'_>) -> Vec<(TypeId, TypeLayout)> {
    l.cx.native
        .structs
        .iter()
        .filter(|(ty, _)| !matches!(l.interner.kind(**ty), TyKind::Tuple(_)))
        .map(|(ty, layout)| (*ty, layout.clone()))
        .collect()
}

fn marshaled_unions(l: &Lcx<'_>) -> Vec<(TypeId, UnionLayout)> {
    l.cx.native
        .unions
        .iter()
        .filter(|(ty, _)| is_marshalable(&l.cx, **ty))
        .map(|(ty, layout)| (*ty, layout.clone()))
        .collect()
}

pub(in super::super) fn register_all(l: &mut Lcx<'_>) {
    if !emits_js_marshal(&l.cx) {
        return;
    }
    let h = l.h();
    let structs = marshaled_structs(l);
    let unions = marshaled_unions(l);
    let values: Vec<(String, bool)> = structs
        .iter()
        .map(|(ty, layout)| (layout.name.clone(), l.interner.is_value_type(*ty)))
        .chain(
            unions
                .iter()
                .map(|(ty, layout)| (layout.name.clone(), l.interner.is_value_union(*ty))),
        )
        .collect();
    for (name, in_place) in values {
        let (to_js, from_js) = struct_syms(&name);
        register(l, &to_js, h.clone(), vec![h.clone()]);
        if in_place {
            register(l, &from_js, Ty::Void, vec![h.clone(), h.clone()]);
        } else {
            register(l, &from_js, h.clone(), vec![h.clone()]);
        }
    }
    for elem in array_elems(&l.cx) {
        let (to_js, from_js) = array_syms(elem);
        register(l, &to_js, h.clone(), vec![h.clone()]);
        register(l, &from_js, h.clone(), vec![h.clone()]);
    }
}

pub(in super::super) fn emit_all(l: &mut Lcx<'_>) {
    if !emits_js_marshal(&l.cx) {
        return;
    }
    for (ty, layout) in marshaled_unions(l) {
        union_to_js(l, ty, &layout);
        js_to_union(l, ty, &layout);
    }
    for (ty, layout) in marshaled_structs(l) {
        struct_to_js(l, &layout);
        js_to_struct(l, ty, &layout);
    }
    for elem in array_elems(&l.cx) {
        array_to_js(l, elem);
        js_to_array(l, elem);
    }
}

impl<'l, 'a> Fx<'l, 'a> {
    fn bridge(&mut self, method: &str, args: &[V]) -> V {
        let name = js_bridge(&self.l.cx, method);
        self.call_v(&name, args)
    }

    fn bridge_do(&mut self, method: &str, args: &[V]) {
        let name = js_bridge(&self.l.cx, method);
        self.call(&name, args);
    }

    fn bridge_str(&mut self, method: &str, j: &V, key: &str) -> V {
        let k = self.str_v(key);
        self.bridge(method, &[j.clone(), k])
    }

    fn load_handle(&mut self, at: &Value) -> V {
        let h = self.h();
        self.load_ty(h, at, MARSHAL_ALIGN, true)
    }

    /// The JS value of the Dream value stored at `at`.
    fn stored_to_js(&mut self, at: &Value, ty: TypeId) -> Option<V> {
        let cx = &self.l.cx;
        match self.interner.kind(ty).clone() {
            TyKind::Prim(p) => {
                let (method, _) = box_prim(p);
                let v = self.load_mem(mem_ty(cx, ty), at, MARSHAL_ALIGN);
                let v = if method == "box_double" {
                    V::s(self.conv(&v, &Ty::F64))
                } else {
                    v
                };
                Some(self.bridge(method, &[v]))
            }
            TyKind::Enum(_) => {
                let v = self.load_ty(Ty::I32, at, MARSHAL_ALIGN, false);
                Some(self.bridge("box_int", &[v]))
            }
            TyKind::Js => Some(self.load_handle(at)),
            TyKind::Array(elem) if is_marshalable(cx, elem) => {
                let a = self.load_handle(at);
                Some(self.call_v(&array_syms(elem).0, &[a]))
            }
            TyKind::Struct(..) => {
                let name = cx.nstruct(ty)?.name.clone();
                let this = if self.interner.is_reference(ty) {
                    self.load_handle(at)
                } else {
                    self.as_ref(&V::s(at.clone()))
                };
                Some(self.call_v(&struct_syms(&name).0, &[this]))
            }
            TyKind::Union(..) if is_marshalable(cx, ty) => {
                let name = cx.nunion(ty)?.name.clone();
                let this = if self.interner.is_value_union(ty) {
                    self.as_ref(&V::s(at.clone()))
                } else {
                    self.load_handle(at)
                };
                Some(self.call_v(&struct_syms(&name).0, &[this]))
            }
            _ => None,
        }
    }

    /// The JS value of a Dream value held in a register (a niche union's payload).
    fn held_to_js(&mut self, v: &V, ty: TypeId) -> Option<V> {
        match self.interner.kind(ty).clone() {
            TyKind::Prim(p) => {
                let (method, _) = box_prim(p);
                let v = if method == "box_double" {
                    V::s(self.conv(v, &Ty::F64))
                } else {
                    v.clone()
                };
                Some(self.bridge(method, &[v]))
            }
            TyKind::Enum(_) => Some(self.bridge("box_int", std::slice::from_ref(v))),
            TyKind::Js => Some(v.clone()),
            TyKind::Array(elem) if is_marshalable(&self.l.cx, elem) => {
                Some(self.call_v(&array_syms(elem).0, std::slice::from_ref(v)))
            }
            TyKind::Struct(..) => {
                let name = self.l.cx.nstruct(ty)?.name.clone();
                Some(self.call_v(&struct_syms(&name).0, std::slice::from_ref(v)))
            }
            TyKind::Union(..) => {
                let name = self.l.cx.nunion(ty)?.name.clone();
                Some(self.call_v(&struct_syms(&name).0, std::slice::from_ref(v)))
            }
            _ => None,
        }
    }

    /// The Dream value of JS value `j` for a type held by handle or register.
    fn value_from_js(&mut self, j: &V, ty: TypeId) -> Option<V> {
        let cx = &self.l.cx;
        match self.interner.kind(ty).clone() {
            TyKind::Prim(p) => {
                let (method, narrow) = unbox_prim(p);
                let v = self.bridge(method, std::slice::from_ref(j));
                Some(if narrow {
                    V::s(self.conv(&v, &Ty::F32))
                } else {
                    v
                })
            }
            TyKind::Enum(_) => Some(self.bridge("as_int", std::slice::from_ref(j))),
            TyKind::Js => Some(j.clone()),
            TyKind::Array(elem) if is_marshalable(cx, elem) => {
                Some(self.call_v(&array_syms(elem).1, std::slice::from_ref(j)))
            }
            TyKind::Struct(..) if self.interner.is_reference(ty) => {
                let name = cx.nstruct(ty)?.name.clone();
                Some(self.call_v(&struct_syms(&name).1, std::slice::from_ref(j)))
            }
            TyKind::Union(..) if is_marshalable(cx, ty) && !self.interner.is_value_union(ty) => {
                let name = cx.nunion(ty)?.name.clone();
                Some(self.call_v(&struct_syms(&name).1, std::slice::from_ref(j)))
            }
            _ => None,
        }
    }

    /// Stores the Dream value of `j` at `dst`; `false` when `ty` does not marshal.
    fn write_from_js(&mut self, dst: &Value, j: &V, ty: TypeId) -> bool {
        let in_place = match self.interner.kind(ty) {
            TyKind::Struct(..) if self.interner.is_value_type(ty) => {
                self.l.cx.nstruct(ty).map(|l| l.name.clone())
            }
            TyKind::Union(..) if self.interner.is_value_union(ty) => {
                self.l.cx.nunion(ty).map(|l| l.name.clone())
            }
            _ => None,
        };
        if let Some(name) = in_place {
            let d = self.as_ref(&V::s(dst.clone()));
            self.call(&struct_syms(&name).1, &[j.clone(), d]);
            return true;
        }
        let Some(v) = self.value_from_js(j, ty) else {
            return false;
        };
        let m = mem_ty(&self.l.cx, ty);
        self.store_mem(m, dst, &v, MARSHAL_ALIGN);
        true
    }

    fn js_is_null(&mut self, j: &V) -> Value {
        let n = self.bridge("host_is_null", std::slice::from_ref(j));
        self.truthy(&n)
    }

    fn write_disc(&mut self, base: &V, disc: i32) {
        let p = self.ptr(base);
        self.store_ty(&Ty::I32, &p, &V::i32(disc as i64), MARSHAL_ALIGN);
    }

    fn load_disc(&mut self, base: &V) -> Value {
        let p = self.ptr(base);
        self.load_ty(Ty::I32, &p, MARSHAL_ALIGN, false).v
    }
}

fn struct_to_js(l: &mut Lcx<'_>, layout: &TypeLayout) {
    let mut fx = glue(l, &struct_syms(&layout.name).0);
    let this = fx.arg(0);
    let o = fx.bridge("object", &[]);
    for f in &layout.fields {
        let at = fx.addr(&this, f.offset as i64);
        let Some(v) = fx.stored_to_js(&at, f.ty) else {
            continue;
        };
        let k = fx.str_v(&f.name);
        fx.bridge_do("set", &[o.clone(), k, v]);
    }
    fx.w.ret(Some(&o.v));
    fx.finish();
}

fn js_to_struct(l: &mut Lcx<'_>, ty: TypeId, layout: &TypeLayout) {
    let in_place = l.interner.is_value_type(ty);
    let tag = l.cx.type_tag(ty);
    let mut fx = glue(l, &struct_syms(&layout.name).1);
    let j = fx.arg(0);
    let base = if in_place {
        fx.arg(1)
    } else {
        fx.call_v(
            "dream_malloc",
            &[V::i32(layout.size as i64), V::i32(tag as i64)],
        )
    };
    for f in &layout.fields {
        let dst = fx.addr(&base, f.offset as i64);
        let jv = fx.bridge_str("get", &j, &f.name);
        if !fx.write_from_js(&dst, &jv, f.ty) {
            let size = native_scalar_size(&fx.l.cx, f.ty).0 as i64;
            fx.memset0(&dst, &Value::i64(size));
        }
    }
    if in_place {
        fx.w.ret(None);
    } else {
        let r = fx.as_ref(&base);
        fx.w.ret(Some(&r.v));
    }
    fx.finish();
}

fn variant<'u>(layout: &'u UnionLayout, name: &str) -> &'u UnionVariant {
    layout
        .variants
        .iter()
        .find(|v| v.name == name)
        .unwrap_or_else(|| crate::internal_error!("Option union missing {name}"))
}

fn niche_payload(layout: &UnionLayout) -> TypeId {
    layout
        .variants
        .iter()
        .find_map(|v| v.fields.first())
        .map(|f| f.ty)
        .unwrap_or_else(|| crate::internal_error!("niche union missing payload field"))
}

fn union_to_js(l: &mut Lcx<'_>, ty: TypeId, layout: &UnionLayout) {
    let niche = l.interner.is_niche_union(ty);
    let mut fx = glue(l, &struct_syms(&layout.name).0);
    let this = fx.arg(0);
    if niche {
        let payload = niche_payload(layout);
        let null = fx.w.icmp("eq", &this.v, &Value::zero(this.ty().clone()));
        let r = fx.if_else_v(
            &null,
            |fx| fx.bridge("host_null", &[]),
            |fx| {
                fx.held_to_js(&this, payload).unwrap_or_else(|| {
                    crate::internal_error!("niche union payload should be marshalable")
                })
            },
        );
        fx.w.ret(Some(&r.v));
        fx.finish();
        return;
    }
    if is_option_union(layout) {
        let none = variant(layout, "None").discriminant;
        let payload = variant(layout, "Some").fields[0].clone();
        let disc = fx.load_disc(&this);
        let is_none = fx.w.icmp("eq", &disc, &Value::i32(none as i64));
        let r = fx.if_else_v(
            &is_none,
            |fx| fx.bridge("host_null", &[]),
            |fx| {
                let at = fx.addr(&this, payload.offset as i64);
                fx.stored_to_js(&at, payload.ty).unwrap_or_else(|| {
                    crate::internal_error!("Option payload should be marshalable")
                })
            },
        );
        fx.w.ret(Some(&r.v));
        fx.finish();
        return;
    }
    let o = fx.bridge("object", &[]);
    let disc = fx.load_disc(&this);
    for v in &layout.variants {
        let hit = fx.w.icmp("eq", &disc, &Value::i32(v.discriminant as i64));
        fx.if_then(&hit, |fx| {
            let name = fx.str_v(&v.name);
            let boxed = fx.bridge("box_string", &[name]);
            let k = fx.str_v("type");
            fx.bridge_do("set", &[o.clone(), k, boxed]);
            for f in &v.fields {
                let at = fx.addr(&this, f.offset as i64);
                if let Some(val) = fx.stored_to_js(&at, f.ty) {
                    let k = fx.str_v(&f.name);
                    fx.bridge_do("set", &[o.clone(), k, val]);
                }
            }
            fx.w.ret(Some(&o.v));
        });
    }
    fx.w.ret(Some(&o.v));
    fx.finish();
}

fn js_to_union(l: &mut Lcx<'_>, ty: TypeId, layout: &UnionLayout) {
    let in_place = l.interner.is_value_union(ty);
    let niche = l.interner.is_niche_union(ty);
    let tag = l.cx.type_tag(ty);
    let mut fx = glue(l, &struct_syms(&layout.name).1);
    let j = fx.arg(0);
    if niche {
        let payload = niche_payload(layout);
        let null = fx.js_is_null(&j);
        let h = fx.h();
        let r = fx.if_else_v(
            &null,
            |_| V::u(Value::zero(h.clone())),
            |fx| {
                fx.value_from_js(&j, payload).unwrap_or_else(|| {
                    crate::internal_error!("niche union payload should be marshalable")
                })
            },
        );
        fx.w.ret(Some(&r.v));
        fx.finish();
        return;
    }
    let base = if in_place {
        fx.arg(1)
    } else {
        fx.call_v(
            "dream_malloc",
            &[V::i32(layout.size as i64), V::i32(tag as i64)],
        )
    };
    let bp = fx.ptr(&base);
    fx.memset0(&bp, &Value::i64(layout.size as i64));
    let finish = |fx: &mut Fx<'_, '_>| {
        if in_place {
            fx.w.ret(None);
        } else {
            let r = fx.as_ref(&base);
            fx.w.ret(Some(&r.v));
        }
    };
    if is_option_union(layout) {
        let none = variant(layout, "None").discriminant;
        let some = variant(layout, "Some");
        let payload = some.fields[0].clone();
        let null = fx.js_is_null(&j);
        let then_b = fx.w.new_block("none");
        let else_b = fx.w.new_block("some");
        let join = fx.w.new_block("join");
        fx.w.cond_br(&null, then_b, else_b);
        fx.w.switch_to(then_b);
        fx.write_disc(&base, none);
        fx.w.br(join);
        fx.w.switch_to(else_b);
        fx.write_disc(&base, some.discriminant);
        let dst = fx.addr(&base, payload.offset as i64);
        if !fx.write_from_js(&dst, &j, payload.ty) {
            crate::internal_error!("Option payload should be marshalable");
        }
        fx.w.br(join);
        fx.w.switch_to(join);
        finish(&mut fx);
        fx.finish();
        return;
    }
    let tv = fx.bridge_str("get", &j, "type");
    let tag_s = fx.bridge("as_string", &[tv]);
    for v in &layout.variants {
        let name = fx.str_v(&v.name);
        let eq = fx.call_v("dream_string_eq", &[tag_s.clone(), name]);
        let hit = fx.truthy(&eq);
        fx.if_then(&hit, |fx| {
            fx.write_disc(&base, v.discriminant);
            for f in &v.fields {
                let jv = fx.bridge_str("get", &j, &f.name);
                let dst = fx.addr(&base, f.offset as i64);
                fx.write_from_js(&dst, &jv, f.ty);
            }
            finish(fx);
        });
    }
    finish(&mut fx);
    fx.finish();
}

/// `for (i = 0; i < n; i++) body(fx, i)` over an `i32` counter.
fn count_loop(fx: &mut Fx<'_, '_>, n: &V, body: impl FnOnce(&mut Fx<'_, '_>, &V)) {
    let slot = fx.w.alloca(Ty::I32, 4);
    fx.w.store(&Value::i32(0), &slot, 4, &[]);
    let head = fx.w.new_block("head");
    let step = fx.w.new_block("step");
    let done = fx.w.new_block("done");
    fx.w.br(head);
    fx.w.switch_to(head);
    let i = V::s(fx.w.load(Ty::I32, &slot, 4, &[]));
    let nv = fx.conv(n, &Ty::I32);
    let lt = fx.w.icmp("slt", &i.v, &nv);
    fx.w.cond_br(&lt, step, done);
    fx.w.switch_to(step);
    body(fx, &i);
    let inc = fx.w.bin("add", &i.v, &Value::i32(1));
    fx.w.store(&inc, &slot, 4, &[]);
    fx.w.br(head);
    fx.w.switch_to(done);
}

fn elem_at(fx: &mut Fx<'_, '_>, arr: &V, i: &V, esize: i64) -> Value {
    let i64v = fx.conv(i, &Ty::I64);
    let off = fx.w.bin("mul", &i64v, &Value::i64(esize));
    let off = fx.w.bin("add", &off, &Value::i64(LEN_PREFIX_SIZE as i64));
    fx.addr_dyn(arr, &off)
}

fn array_to_js(l: &mut Lcx<'_>, elem: TypeId) {
    let esize = elem_size(&l.cx, elem) as i64;
    let mut fx = glue(l, &array_syms(elem).0);
    let arr = fx.arg(0);
    let o = fx.bridge("array", &[]);
    let nz = fx.truthy(&arr);
    let n = fx.if_else_v(
        &nz,
        |fx| {
            let p = fx.ptr(&arr);
            fx.load_ty(Ty::I32, &p, align_at(&Ty::I32, 0), false)
        },
        |_| V::i32(0),
    );
    count_loop(&mut fx, &n, |fx, i| {
        let at = elem_at(fx, &arr, i, esize);
        let v = fx
            .stored_to_js(&at, elem)
            .unwrap_or_else(|| crate::internal_error!("array element should be marshalable"));
        let idx = fx.bridge("box_int", std::slice::from_ref(i));
        fx.bridge_do("index_set", &[o.clone(), idx, v]);
    });
    fx.w.ret(Some(&o.v));
    fx.finish();
}

fn js_to_array(l: &mut Lcx<'_>, elem: TypeId) {
    let esize = elem_size(&l.cx, elem) as i64;
    let mut fx = glue(l, &array_syms(elem).1);
    let j = fx.arg(0);
    let len = fx.bridge_str("get", &j, "length");
    let n = fx.bridge("as_int", &[len]);
    let o = fx.call_v("dream_array_new", &[n.clone(), V::i32(esize)]);
    count_loop(&mut fx, &n, |fx, i| {
        let idx = fx.bridge("box_int", std::slice::from_ref(i));
        let jv = fx.bridge("index_get", &[j.clone(), idx]);
        let dst = elem_at(fx, &o, i, esize);
        if !fx.write_from_js(&dst, &jv, elem) {
            crate::internal_error!("array element should be marshalable");
        }
    });
    let r = fx.as_ref(&o);
    fx.w.ret(Some(&r.v));
    fx.finish();
}
