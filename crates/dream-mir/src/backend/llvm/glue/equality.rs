//! Generated structural `==` for value structs and data enums.
//!
//! A `{Type}_value_eq(a, b)` body is requested the first time a function compares two values of
//! that type, and every requested body is emitted after the functions, including the ones its own
//! fields request.

use super::super::fx::{Fx, V};
use super::super::ir::{FnTy, Ty, Value};
use super::super::lcx::{FnSig, Lcx};
use super::glue;
use crate::backend::shared::abi_types::c_ident;
use dream_types::{PrimTy, TyKind, TypeId};

/// Value structs, tuples, and data enums compare field by field. Classes, arrays, and interfaces
/// keep reference identity.
pub(in super::super) fn has_structural_eq(l: &Lcx<'_>, ty: TypeId) -> bool {
    l.cx.nunion(ty).is_some() || (l.cx.nstruct(ty).is_some() && l.interner.is_value_type(ty))
}

fn request(l: &mut Lcx<'_>, ty: TypeId) -> String {
    let layout_name = match (l.cx.nstruct(ty), l.cx.nunion(ty)) {
        (Some(s), _) => s.name.clone(),
        (None, Some(u)) => u.name.clone(),
        (None, None) => crate::internal_error!("structural equality of unlaid type {ty:?}"),
    };
    let name = c_ident(&format!("{layout_name}_value_eq"));
    if l.value_eq.insert(ty) {
        let h = l.h();
        l.own(&name, FnSig::plain(FnTy::new(Ty::I32, vec![h.clone(), h])));
    }
    name
}

pub(in super::super) fn emit_all(l: &mut Lcx<'_>) {
    let mut emitted = 0;
    while let Some(&ty) = l.value_eq.get_index(emitted) {
        let name = request(l, ty);
        if l.cx.nstruct(ty).is_some() {
            struct_eq(l, ty, &name);
        } else {
            union_eq(l, ty, &name);
        }
        emitted += 1;
    }
}

impl<'l, 'a> Fx<'l, 'a> {
    /// `a == b` compared by value as an `i32` boolean, or `None` when `ty` keeps identity equality.
    pub(in super::super) fn structural_eq(&mut self, ty: TypeId, a: &V, b: &V) -> Option<V> {
        if !has_structural_eq(self.l, ty) {
            return None;
        }
        let name = request(self.l, ty);
        Some(self.call_v(&name, &[a.clone(), b.clone()]))
    }

    fn field_eq(&mut self, ty: TypeId, x: &V, y: &V) -> Value {
        match self.interner.kind(ty) {
            TyKind::Prim(PrimTy::String) => {
                let r = self.call_v("dream_string_eq", &[x.clone(), y.clone()]);
                self.truthy(&r)
            }
            TyKind::Prim(PrimTy::Float | PrimTy::Float16 | PrimTy::Double) => {
                self.w.fcmp("oeq", &x.v, &y.v)
            }
            _ => match self.structural_eq(ty, x, y) {
                Some(r) => self.truthy(&r),
                None => self.w.icmp("eq", &x.v, &y.v),
            },
        }
    }

    fn ret_bool(&mut self, b: &Value) {
        let r = self.w.cast("zext", b, Ty::I32);
        self.w.ret(Some(&r));
    }
}

fn struct_eq(l: &mut Lcx<'_>, ty: TypeId, name: &str) {
    let layout =
        l.cx.nstruct(ty)
            .cloned()
            .unwrap_or_else(|| crate::internal_error!("equality of unlaid struct {ty:?}"));
    let mut fx = glue(l, name);
    let (a, b) = (fx.arg(0), fx.arg(1));
    let mut all = Value::i1(true);
    for f in &layout.fields {
        let x = fx.proto_field(&a, f);
        let y = fx.proto_field(&b, f);
        let e = fx.field_eq(f.ty, &x, &y);
        all = fx.w.bin("and", &all, &e);
    }
    fx.ret_bool(&all);
    fx.finish();
}

fn union_eq(l: &mut Lcx<'_>, ty: TypeId, name: &str) {
    let layout =
        l.cx.nunion(ty)
            .cloned()
            .unwrap_or_else(|| crate::internal_error!("equality of unlaid union {ty:?}"));
    let niche = l.cx.niche_variant_discriminants(ty).is_some();
    let mut fx = glue(l, name);
    let (a, b) = (fx.arg(0), fx.arg(1));
    let same = fx.w.icmp("eq", &a.v, &b.v);
    fx.if_then(&same, |fx| fx.w.ret(Some(&Value::i32(1))));
    let null = Value::zero(a.ty().clone());
    let a_null = fx.w.icmp("eq", &a.v, &null);
    let b_null = fx.w.icmp("eq", &b.v, &null);
    let either_null = fx.w.bin("or", &a_null, &b_null);
    fx.if_then(&either_null, |fx| fx.w.ret(Some(&Value::i32(0))));
    if niche {
        // A niche union is its payload pointer; the one payload field is the value itself.
        let payload = layout
            .variants
            .iter()
            .find_map(|v| v.fields.first())
            .unwrap_or_else(|| crate::internal_error!("niche union {ty:?} without a payload"));
        let e = fx.field_eq(payload.ty, &a, &b);
        fx.ret_bool(&e);
        fx.finish();
        return;
    }
    let (pa, pb) = (fx.ptr(&a), fx.ptr(&b));
    let da = fx.load_ty(Ty::I32, &pa, 4, false);
    let db = fx.load_ty(Ty::I32, &pb, 4, false);
    let differ = fx.w.icmp("ne", &da.v, &db.v);
    fx.if_then(&differ, |fx| fx.w.ret(Some(&Value::i32(0))));
    let equal = fx.w.new_block("equal");
    let mut arms = Vec::new();
    let mut bodies = Vec::new();
    for v in &layout.variants {
        if v.fields.is_empty() || arms.iter().any(|(k, _)| *k == v.discriminant as i128) {
            continue;
        }
        let block = fx.w.new_block("variant");
        arms.push((v.discriminant as i128, block));
        bodies.push((block, v.clone()));
    }
    fx.w.switch(&da.v, equal, &arms);
    for (block, v) in bodies {
        fx.w.switch_to(block);
        let mut all = Value::i1(true);
        for f in &v.fields {
            let x = fx.proto_field(&a, f);
            let y = fx.proto_field(&b, f);
            let e = fx.field_eq(f.ty, &x, &y);
            all = fx.w.bin("and", &all, &e);
        }
        fx.ret_bool(&all);
    }
    fx.w.switch_to(equal);
    fx.w.ret(Some(&Value::i32(1)));
    fx.finish();
}
