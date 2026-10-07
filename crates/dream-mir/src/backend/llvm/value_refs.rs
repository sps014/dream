//! Typed traversal and copying of owned references inside inline values.

use super::fx::{Fx, V};
use super::ir::{Ty, Value};
use super::places::ELEM_ALIGN;
use crate::Rvalue;
use crate::backend::shared::abi_types::elem_size;
use crate::backend::shared::glue::{release_sym, retain_sym};
use crate::rc_store::rvalue_allocates;
use dream_types::TypeId;

#[derive(Clone)]
enum RefAction {
    Retain,
    Visit,
    Release,
    Clear,
    Publish(V),
}

impl Fx<'_, '_> {
    /// Copies a value struct into `dest`, dropping the old contents when asked and taking the
    /// nested references (a fresh heap box is freed instead: its references move with the bytes).
    pub fn memcpy_value(
        &mut self,
        rv: &Rvalue,
        rhs: &V,
        ty: TypeId,
        dest: &V,
        retain_copy: bool,
        drop_old: bool,
    ) {
        let size = elem_size(&self.l.cx, ty) as i64;
        if drop_old {
            self.value_refs(ty, dest, false);
        }
        let (d, s) = (self.ptr(dest), self.ptr(rhs));
        self.memcpy(&d, &s, &Value::i64(size));
        if rvalue_allocates(rv) {
            if !self.is_buffer_result(rv) {
                self.call("dream_free", std::slice::from_ref(rhs));
            }
        } else if retain_copy {
            self.value_refs(ty, dest, true);
        }
    }

    /// Retains or releases every owned reference nested in the value at `base`.
    pub fn value_refs(&mut self, ty: TypeId, base: &V, retain: bool) {
        let action = if retain {
            RefAction::Retain
        } else {
            RefAction::Release
        };
        self.walk_value_refs(ty, base, &action);
    }

    pub(super) fn clear_refs(&mut self, ty: TypeId, base: &V) {
        self.walk_value_refs(ty, base, &RefAction::Clear);
    }

    pub(super) fn publish_refs(&mut self, ty: TypeId, base: &V, owner: &V) {
        let action = RefAction::Publish(owner.clone());
        if self.is_value(ty) {
            self.walk_value_refs(ty, base, &action);
        } else if self.is_rc(ty) && self.store_checked {
            let child = self.as_ref(base);
            self.call("dream_publish_edge", &[owner.clone(), child]);
        } else if self.is_rc(ty) {
            self.ref_action(ty, base, &action);
        }
    }

    pub(super) fn visit_refs(&mut self, ty: TypeId, base: &V) {
        self.walk_value_refs(ty, base, &RefAction::Visit);
    }

    fn walk_value_refs(&mut self, ty: TypeId, base: &V, action: &RefAction) {
        if let Some(layout) = self.l.cx.nstruct(ty).cloned() {
            for field in &layout.fields {
                self.value_field_ref(field, base, action);
            }
            return;
        }
        let Some(u) = self.l.cx.nunion(ty).cloned() else {
            return;
        };
        let bp = self.ptr(base);
        let disc = self.load_ty(Ty::I32, &bp, 4, false);
        let join = self.w.new_block("vr.join");
        let mut arms = Vec::new();
        let mut bodies = Vec::new();
        for variant in &u.variants {
            let owns = variant
                .fields
                .iter()
                .any(|f| !(f.is_weak || f.is_unowned) && (self.is_value(f.ty) || self.is_rc(f.ty)));
            if !owns {
                continue;
            }
            let b = self.w.new_block("vr.arm");
            arms.push((variant.discriminant as i128, b));
            bodies.push((b, variant.clone()));
        }
        if arms.is_empty() {
            self.w.br(join);
            self.w.switch_to(join);
            return;
        }
        self.w.switch(&disc.v, join, &arms);
        for (b, variant) in bodies {
            self.w.switch_to(b);
            for field in &variant.fields {
                self.value_field_ref(field, base, action);
            }
            self.w.br(join);
        }
        self.w.switch_to(join);
    }

    fn value_field_ref(&mut self, field: &dream_hir::FieldLayout, base: &V, action: &RefAction) {
        if field.is_weak || field.is_unowned {
            return;
        }
        let off = field.offset as i64;
        if self.is_value(field.ty) {
            let at = self.addr(base, off);
            let at = self.ptr_value(&at);
            self.walk_value_refs(field.ty, &at, action);
        } else if self.is_rc(field.ty) {
            let at = self.addr(base, off);
            let v = self.load_ty(self.h(), &at, ELEM_ALIGN, true);
            if matches!(action, RefAction::Clear) {
                let h = self.h();
                self.store_ty(&h, &at, &V::s(Value::zero(h.clone())), ELEM_ALIGN);
            }
            self.ref_action(field.ty, &v, action);
        }
    }

    fn ref_action(&mut self, ty: TypeId, value: &V, action: &RefAction) {
        match action {
            RefAction::Visit => {
                let value = self.as_ref(value);
                self.call("dream_visit_edge", &[value]);
            }
            RefAction::Retain => {
                self.call(retain_sym(&self.l.cx, ty), std::slice::from_ref(value));
            }
            RefAction::Release | RefAction::Clear => {
                self.call(&release_sym(&self.l.cx, ty), std::slice::from_ref(value));
            }
            RefAction::Publish(owner) => {
                let child = self.as_ref(value);
                self.call("dream_publish_child", &[owner.clone(), child]);
            }
        }
    }
}
