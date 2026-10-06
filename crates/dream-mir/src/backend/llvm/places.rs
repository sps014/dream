//! Operand reads and place stores, including the ownership-aware container stores (`rc_store`,
//! unowned and weak slots) whose policy lives in `shared::place_policy`.

use std::convert::TryFrom;

use super::fx::{align_at, mem_ll, Fx, V};
use super::ir::{Ty, Value};
use crate::backend::shared::abi_types::{array_elem_ty, elem_size, mem_ty, MemTy};
use crate::backend::shared::glue::{release_sym, retain_sym};
use crate::backend::shared::panic_msgs;
use crate::backend::shared::place_policy::{
    borrowed_ref_store, is_value_place_alias, realloc_self_store, unique_move_src,
};
use crate::rc_store::rvalue_allocates;
use crate::{Const, Global, Local, Operand, Place, Rvalue};
use dream_types::{TyKind, TypeId};

/// Array payload elements sit at `data + 4 + i * es`, so nothing wider than 4 is guaranteed.
pub(super) const ELEM_ALIGN: u32 = 4;

impl<'l, 'a> Fx<'l, 'a> {
    // ---- locals and globals -------------------------------------------------------------------

    pub fn read_local(&mut self, l: Local) -> V {
        let Some(s) = &self.slots[l.0 as usize] else {
            return V::i32(0);
        };
        let (ptr, ty, unsigned) = (s.ptr.clone(), s.ty.clone(), s.unsigned);
        self.load_ty(ty, &ptr, 8, unsigned)
    }

    pub fn write_local(&mut self, l: Local, x: &V) {
        let Some(s) = &self.slots[l.0 as usize] else {
            return;
        };
        let (ptr, ty) = (s.ptr.clone(), s.ty.clone());
        self.store_ty(&ty, &ptr, x, 8);
    }

    pub fn global_ty(&self, g: Global) -> TypeId {
        self.l
            .cx
            .global_ty(g)
            .unwrap_or_else(|| self.interner.int())
    }

    pub(super) fn global_ll(&self, g: Global) -> (Ty, bool) {
        let ty = self.global_ty(g);
        if g.0 == 0 || self.is_value(ty) {
            return (self.h(), true);
        }
        (
            super::types::ll_ty(self.interner, ty, &self.h(), &self.word()),
            super::types::is_unsigned(self.interner, ty),
        )
    }

    pub(super) fn read_global(&mut self, g: Global) -> V {
        if g.0 == 0 && self.l.cx.target.spec().capabilities.linear_memory {
            return self.call_v("dream_g0_get", &[]);
        }
        let (ty, unsigned) = self.global_ll(g);
        let p = Value::global(format!("g{}", g.0));
        self.load_ty(ty, &p, 8, unsigned)
    }

    pub(super) fn write_global(&mut self, g: Global, x: &V) {
        if g.0 == 0 && self.l.cx.target.spec().capabilities.linear_memory {
            self.call("dream_g0_set", std::slice::from_ref(x));
            return;
        }
        let (ty, _) = self.global_ll(g);
        let p = Value::global(format!("g{}", g.0));
        self.store_ty(&ty, &p, x, 8);
    }

    // ---- operands -----------------------------------------------------------------------------

    pub fn operand_ty(&self, o: &Operand) -> TypeId {
        let i = self.interner;
        match o {
            Operand::Copy(Place::Local(l)) => self.f.local_ty(*l),
            Operand::Copy(Place::Global(g)) => self.global_ty(*g),
            Operand::Copy(Place::Field { base, field }) => self
                .l
                .cx
                .nstruct(self.f.local_ty(*base))
                .and_then(|layout| layout.fields.get(*field))
                .map(|field| field.ty)
                .unwrap_or_else(|| self.f.local_ty(*base)),
            Operand::Copy(Place::Index { base, .. }) => array_elem_ty(i, self.f.local_ty(*base)),
            Operand::Copy(Place::Deref { elem_ty, .. }) => *elem_ty,
            Operand::Const(Const::Str(_)) => i.string(),
            Operand::Const(Const::Long(_)) => i.long(),
            Operand::Const(Const::Float(_)) => i.double(),
            Operand::Const(Const::F32(_)) => i.float(),
            Operand::Const(Const::Bool(_)) => i.bool(),
            Operand::Const(Const::Char(_)) => i.char(),
            Operand::Const(_) => i.int(),
        }
    }

    pub fn operand(&mut self, o: &Operand) -> V {
        match o {
            Operand::Copy(p) => self.load_place(p),
            Operand::Const(c) => match c {
                Const::Int(v) => match i32::try_from(*v) {
                    Ok(n) => V::i32(n as i64),
                    Err(_) => V::i64(*v),
                },
                Const::Long(v) => V::i64(*v),
                Const::Float(v) => V::s(Value::f64(*v)),
                Const::F32(v) => V::s(Value::f32(*v)),
                Const::Bool(b) => V::i32(*b as i64),
                Const::Char(ch) => V::i32(*ch as i64),
                Const::Str(s) => self.str_v(s),
                Const::Null => V::s(Value::zero(self.h())),
            },
        }
    }

    /// The address of `base`'s field at `off`; value-typed bases may live inside an array
    /// payload, so only heap references are known to be 8-aligned.
    fn field_addr(&mut self, base: Local, off: u32) -> (Value, u32) {
        let b = self.read_local(base);
        let addr = self.addr(&b, off as i64);
        let cap = if self.is_value(self.f.local_ty(base)) {
            ELEM_ALIGN
        } else {
            8
        };
        (addr, align_at(&self.h(), off as i64).min(cap).max(1))
    }

    pub(super) fn ptr_value(&mut self, addr: &Value) -> V {
        self.as_ref(&V::s(addr.clone()))
    }

    pub fn load_place(&mut self, place: &Place) -> V {
        match place {
            Place::Local(l) => self.read_local(*l),
            Place::Global(g) => self.read_global(*g),
            Place::Field { base, field } => {
                let ty = self.f.local_ty(*base);
                let Some(layout) = self.l.cx.nstruct(ty) else {
                    let b = self.read_local(*base);
                    let ps = self.l.cx.target.abi().ptr_size as i64;
                    let addr = self.addr(&b, *field as i64 * ps);
                    return self.load_mem(MemTy::Ptr, &addr, ELEM_ALIGN);
                };
                let fld = layout.fields.get(*field).cloned().unwrap_or_else(|| {
                    crate::internal_error!("missing field {field} on type {ty:?}")
                });
                let (addr, align) = self.field_addr(*base, fld.offset);
                if self.is_value(fld.ty) {
                    return self.ptr_value(&addr);
                }
                let m = mem_ty(&self.l.cx, fld.ty);
                let v = self.load_mem(m, &addr, align);
                if fld.is_unowned {
                    self.check_unowned(&v);
                }
                v
            }
            Place::Index {
                base,
                index,
                unchecked,
            } => {
                let ety = array_elem_ty(self.interner, self.f.local_ty(*base));
                let es = elem_size(&self.l.cx, ety);
                let addr = self.index_addr(*base, index, es, *unchecked);
                if self.is_value(ety) {
                    self.ptr_value(&addr)
                } else {
                    let m = mem_ty(&self.l.cx, ety);
                    self.load_mem(m, &addr, ELEM_ALIGN)
                }
            }
            Place::Deref { ptr, elem_ty } => {
                if self.is_value(*elem_ty) {
                    return self.read_local(*ptr);
                }
                let p = self.read_local(*ptr);
                let addr = self.ptr(&p);
                let m = mem_ty(&self.l.cx, *elem_ty);
                self.load_mem(m, &addr, ELEM_ALIGN)
            }
        }
    }

    /// Null = never assigned; the poison sentinel is written when the referent is destroyed.
    fn check_unowned(&mut self, v: &V) {
        let is_null = self.w.icmp("eq", &v.v, &Value::zero(v.ty().clone()));
        self.if_then(&is_null, |fx| fx.panic_with(panic_msgs::UNOWNED_NULL_DEREF));
        let poison = self.conv(&V::i32(crate::abi::UNOWNED_POISON as i64), v.ty());
        let dead = self.w.icmp("eq", &v.v, &poison);
        self.if_then(&dead, |fx| fx.panic_with(panic_msgs::UNOWNED_DESTROYED));
    }

    pub fn index_addr(&mut self, base: Local, index: &Operand, es: u32, unchecked: bool) -> Value {
        let idx = self.operand(index);
        let b = self.read_local(base);
        let is_array = matches!(self.interner.kind(self.f.local_ty(base)), TyKind::Array(_));
        if unchecked || !is_array {
            let word = self.word();
            let i = self.conv(&idx, &word);
            let off = self.w.bin("mul", &i, &Value::int(word, es as i128));
            let data = self.addr(&b, crate::abi::LEN_PREFIX_SIZE as i64);
            return self.w.gep_i8(&data, &off);
        }
        let msg = self.str_v(panic_msgs::INDEX_OUT_OF_BOUNDS);
        let at = self.panic_location();
        let i = V::s(self.conv(&idx, &Ty::I64));
        self.call_v("dream_array_at", &[b, i, V::i32(es as i64), msg, at])
            .v
    }

    // ---- stores -------------------------------------------------------------------------------

    pub fn store(&mut self, place: &Place, rv: &Rvalue, rhs: V) {
        match place {
            Place::Local(l) if self.is_value(self.f.local_ty(*l)) => {
                let lty = self.f.local_ty(*l);
                if is_value_place_alias(self.f, *l, rv) {
                    let r = self.as_ref(&rhs);
                    self.write_local(*l, &r);
                    return;
                }
                if let Rvalue::Use(Operand::Copy(Place::Local(src))) = rv {
                    let src_ty = self.f.local_ty(*src);
                    let same_size = self
                        .l
                        .cx
                        .nstruct(src_ty)
                        .is_some_and(|layout| layout.size == elem_size(&self.l.cx, lty));
                    if !self.is_value(src_ty) && same_size {
                        let v = self.read_local(*src);
                        self.write_local(*l, &v);
                        return;
                    }
                }
                let retain_copy = matches!(
                    rv,
                    Rvalue::Use(Operand::Copy(
                        Place::Field { .. } | Place::Index { .. } | Place::Deref { .. }
                    )) | Rvalue::UnionField { .. }
                );
                let dest = self.read_local(*l);
                self.memcpy_value(rv, &rhs, lty, &dest, retain_copy, false);
            }
            Place::Local(l) => self.write_local(*l, &rhs),
            Place::Global(g) => self.store_global(*g, place, rv, rhs),
            Place::Field { base, field } => self.store_field(*base, *field, place, rv, rhs),
            Place::Index {
                base,
                index,
                unchecked,
            } => {
                let ety = array_elem_ty(self.interner, self.f.local_ty(*base));
                let es = elem_size(&self.l.cx, ety);
                let addr = self.index_addr(*base, index, es, *unchecked);
                if self.is_value(ety) {
                    let src = self.ptr(&rhs);
                    self.publish_store(place, ety, &rhs);
                    self.memcpy(&addr, &src, &Value::i64(es as i64));
                    if rvalue_allocates(rv) && !self.is_buffer_result(rv) {
                        self.call("dream_free", &[rhs]);
                    }
                    return;
                }
                let m = mem_ty(&self.l.cx, ety);
                self.publish_store(place, ety, &rhs);
                if !realloc_self_store(place, rv) && self.interner.is_reference(ety) {
                    self.rc_store_ty(ety, &addr, &rhs, rv, ELEM_ALIGN);
                    return;
                }
                self.store_mem(m, &addr, &rhs, ELEM_ALIGN);
            }
            Place::Deref { ptr, elem_ty } => {
                let p = self.read_local(*ptr);
                let dst = self.ptr(&p);
                self.publish_store(place, *elem_ty, &rhs);
                if self.is_value(*elem_ty) {
                    let size = elem_size(&self.l.cx, *elem_ty);
                    let src = self.ptr(&rhs);
                    self.memcpy(&dst, &src, &Value::i64(size as i64));
                    return;
                }
                let m = mem_ty(&self.l.cx, *elem_ty);
                self.store_mem(m, &dst, &rhs, ELEM_ALIGN);
            }
        }
    }

    fn store_global(&mut self, g: Global, place: &Place, rv: &Rvalue, rhs: V) {
        let ty = self.global_ty(g);
        self.publish_store(place, ty, &rhs);
        if self.is_value(ty) {
            let alias = matches!(rv, Rvalue::Use(Operand::Copy(Place::Global(src))) if *src == g);
            let dest = self.read_global(g);
            self.memcpy_value(rv, &rhs, ty, &dest, true, !alias);
            return;
        }
        if self.is_rc(ty) && !realloc_self_store(place, rv) {
            let slot = Value::global(format!("g{}", g.0));
            self.rc_store_ty(ty, &slot, &rhs, rv, 8);
            return;
        }
        self.write_global(g, &rhs);
    }

    fn store_field(&mut self, base: Local, field: usize, place: &Place, rv: &Rvalue, rhs: V) {
        let ty = self.f.local_ty(base);
        let Some(layout) = self.l.cx.nstruct(ty) else {
            let b = self.read_local(base);
            let ps = self.l.cx.target.abi().ptr_size as i64;
            let addr = self.addr(&b, field as i64 * ps);
            self.store_mem(MemTy::Ptr, &addr, &rhs, ELEM_ALIGN);
            return;
        };
        let fld = layout
            .fields
            .get(field)
            .cloned()
            .unwrap_or_else(|| crate::internal_error!("missing field {field} on type {ty:?}"));
        let (slot, align) = self.field_addr(base, fld.offset);
        if !fld.is_unowned && !fld.is_weak {
            self.publish_store(place, fld.ty, &rhs);
        }
        if self.is_value(fld.ty) {
            let alias = matches!(
                rv,
                Rvalue::Use(Operand::Copy(Place::Field { base: b, field: f }))
                    if *b == base && *f == field
            );
            let dest = self.ptr_value(&slot);
            self.memcpy_value(rv, &rhs, fld.ty, &dest, true, !alias);
            return;
        }
        let m = mem_ty(&self.l.cx, fld.ty);
        if realloc_self_store(place, rv) {
            self.store_mem(m, &slot, &rhs, align);
            return;
        }
        if fld.is_unowned {
            self.unowned_store(&slot, &rhs, align);
            return;
        }
        if fld.is_weak {
            self.weak_option_store(&slot, &fld, rv, &rhs, align);
            return;
        }
        if self.interner.is_reference(fld.ty) && !self.f.locals[base.0 as usize].borrows_refs {
            self.rc_store_ty(fld.ty, &slot, &rhs, rv, align);
            return;
        }
        self.store_mem(m, &slot, &rhs, align);
    }

    /// Stores a reference into an owning slot: release the old value, adopting or retaining the
    /// new one as `shared::place_policy` decides, then null a uniquely moved source local.
    fn rc_store_ty(&mut self, ty: TypeId, slot: &Value, rhs: &V, rv: &Rvalue, align: u32) {
        let release = release_sym(&self.l.cx, ty);
        let retain = retain_sym(&self.l.cx, ty);
        let move_id = unique_move_src(rv);
        let borrowed = borrowed_ref_store(self.interner, rv) && move_id.is_none();
        let (mty, _) = mem_ll(MemTy::Ptr, &self.h(), &self.word());
        let old = self.load_ty(mty.clone(), slot, align, true);
        let v = self.as_ref(rhs);
        if borrowed {
            let changed = self.w.icmp("ne", &old.v, &v.v);
            self.if_then(&changed, |fx| {
                fx.call(retain, std::slice::from_ref(&v));
                fx.store_ty(&mty, slot, &v, align);
                fx.call(&release, std::slice::from_ref(&old));
            });
        } else {
            self.store_ty(&mty, slot, &v, align);
            self.call(&release, &[old]);
        }
        if let Some(id) = move_id {
            self.write_local(Local(id), &V::s(Value::zero(self.h())));
        }
    }

    fn unowned_store(&mut self, slot: &Value, rhs: &V, align: u32) {
        let old = self.load_ty(self.h(), slot, align, true);
        let slot_ref = self.ptr_value(slot);
        let nz = self.truthy(&old);
        self.if_then(&nz, |fx| {
            fx.call("dream_weak_unregister", &[old.clone(), slot_ref.clone()]);
        });
        let new = self.as_ref(rhs);
        self.store_ty(&self.h(), slot, &new, align);
        let nz = self.truthy(&new);
        self.if_then(&nz, |fx| {
            fx.call(
                "dream_weak_register",
                &[new.clone(), slot_ref.clone(), V::i32(1), V::i32(0)],
            );
        });
    }

    fn weak_option_store(
        &mut self,
        slot: &Value,
        fld: &dream_hir::FieldLayout,
        rv: &Rvalue,
        rhs: &V,
        align: u32,
    ) {
        let slot_ref = self.ptr_value(slot);
        // Niche-encoded weak field: the slot holds the raw payload pointer and is registered with
        // kind 2, so the runtime nulls it when the referent dies.
        if self.interner.is_niche_union(fld.ty) {
            let retain_copy = unique_move_src(rv).is_none();
            let new = self.as_ref(rhs);
            let old = self.load_ty(self.h(), slot, align, true);
            self.store_ty(&self.h(), slot, &new, align);
            let nz = self.truthy(&old);
            self.if_then(&nz, |fx| {
                fx.call("dream_weak_unregister", &[old.clone(), slot_ref.clone()]);
            });
            let nz = self.truthy(&new);
            self.if_then(&nz, |fx| {
                fx.call(
                    "dream_weak_register",
                    &[new.clone(), slot_ref.clone(), V::i32(2), V::i32(0)],
                );
            });
            if retain_copy {
                self.call("dream_release", &[new]);
            }
            return;
        }
        let Some(u) = self.l.cx.nunion(fld.ty).cloned() else {
            self.store_ty(&self.h(), slot, rhs, align);
            return;
        };
        let some = u.variant("Some").map(|v| v.discriminant).unwrap_or(0);
        let none = u.variant("None").map(|v| v.discriminant).unwrap_or(1);
        let poff = u
            .variant("Some")
            .and_then(|v| v.fields.first())
            .map(|f| f.offset)
            .unwrap_or(8) as i64;
        let size = u.size.max(16) as i64;
        let drop_src = rvalue_allocates(rv).then(|| release_sym(&self.l.cx, fld.ty));
        let src = self.as_ref(rhs);
        let old = self.load_ty(self.h(), slot, align, true);
        let box_ = self.call_v("dream_malloc", &[V::i64(size), V::i32(0)]);
        let (bp, sp) = (self.ptr(&box_), self.ptr(&src));
        self.memcpy(&bp, &sp, &Value::i64(size));
        let disc = self.load_ty(Ty::I32, &sp, 4, false);
        let is_some = self.w.icmp("eq", &disc.v, &Value::i32(some as i64));
        self.if_then(&is_some, |fx| {
            let pa = fx.addr(&src, poff);
            let payload = fx.load_ty(fx.h(), &pa, ELEM_ALIGN, true);
            fx.call(
                "dream_weak_register",
                &[payload, box_.clone(), V::i32(0), V::i32(none as i64)],
            );
        });
        self.store_ty(&self.h(), slot, &box_, align);
        if let Some(rel) = drop_src {
            self.call(&rel, &[src]);
        }
        let nz = self.truthy(&old);
        self.if_then(&nz, |fx| {
            let op = fx.ptr(&old);
            let d = fx.load_ty(Ty::I32, &op, 4, false);
            let was_some = fx.w.icmp("eq", &d.v, &Value::i32(some as i64));
            fx.if_then(&was_some, |fx| {
                let pa = fx.addr(&old, poff);
                let payload = fx.load_ty(fx.h(), &pa, 8, true);
                fx.call("dream_weak_unregister", &[payload, old.clone()]);
            });
            fx.call("dream_free", std::slice::from_ref(&old));
        });
    }
}
