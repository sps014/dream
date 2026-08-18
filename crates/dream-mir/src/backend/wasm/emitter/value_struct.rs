//! The value(`struct`)/value-union inline (non-heap) representation for the WAT backend: the
//! per-function shadow-frame prologue/teardown, in-place construction (`New`/`UnionNew`), byte-wise
//! copy/drop, and sret-call helpers. Split out of `emitter.rs`; these are methods on the parent's
//! private `Emitter`.

use super::*;

impl Emitter<'_> {
    /// Reserves this function's shadow-stack frame (for inline value(`struct`) locals): save `$__sp`,
    /// carve the frame by growing the stack downward, zero it (so drop-glue on a not-yet-assigned slot
    /// sees null reference fields), and point each owning value local at its slot.
    pub(super) fn emit_value_frame_prologue(&mut self) {
        if self.frame.size == 0 {
            return;
        }
        let size = self.frame.size;
        self.f.global_get("__sp");
        self.f.local_set("__saved_sp");
        self.f.global_get("__sp");
        self.f.i32_const(size as i32);
        self.f.i32_sub();
        self.f.global_set("__sp");
        // Zero the whole frame: memory.fill(dest = $__sp, value = 0, len = size).
        self.f.global_get("__sp");
        self.f.i32_const(0);
        self.f.i32_const(size as i32);
        self.f.memory_fill();
        for (local, offset) in self.frame.owning_slots() {
            let ty = self.func.local_ty(local);
            let l0 = local.0;
            let slot_addr = |s: &mut Self| {
                s.f.global_get("__sp");
                if offset > 0 {
                    s.f.i32_const((offset) as i32);
                    s.f.i32_add();
                }
            };
            if self.frame.kind(local) == Some(ValueLocalKind::Param) {
                // A value param arrives as a pointer to the caller's value: copy those bytes into the
                // callee's private slot (retaining reference fields), then rebind the param to the slot
                // so the caller's value is never mutated (copy semantics).
                self.emit_value_copy(slot_addr, |s| s.f.local_get(&(l0).to_string()), ty, false);
            }
            slot_addr(self);
            self.f.local_set(&(l0).to_string());
        }
    }

    /// True when this function returns a value struct by the sret ABI (a hidden `$__sret` pointer)
    /// rather than as a WASM result.
    pub(super) fn returns_value_struct(&self) -> bool {
        self.interner.is_value_type(self.func.ret)
    }

    /// True when this function returns an ordinary WASM value (non-void, non-value-struct).
    pub(super) fn wasm_returns_value(&self) -> bool {
        !matches!(self.interner.kind(self.func.ret), TyKind::Void) && !self.returns_value_struct()
    }

    /// The inline byte size of value struct `ty`.
    pub(super) fn value_size(&self, ty: TypeId) -> u32 {
        scalar_size(self.interner, ty).0
    }

    /// True when value struct `ty` needs retain/drop glue (embeds references or declares `del`).
    pub(super) fn value_has_glue(&self, ty: TypeId) -> bool {
        self.value_glue.contains(&ty)
    }

    /// The layout name of value type `ty` (a value struct or value union), if laid out. Used to name
    /// its retain/drop glue.
    pub(super) fn value_name(&self, ty: TypeId) -> Option<String> {
        let stripped = ty;
        if let Some(l) = self.layouts.get(stripped) {
            return Some(l.name.clone());
        }
        self.layouts.union(stripped).map(|u| u.name.clone())
    }

    /// Pushes the address of value place `p` (a value struct is addressed, never loaded).
    fn emit_place_addr(&mut self, p: &Place) {
        match p {
            Place::Local(l) => self.f.local_get(&(l.0).to_string()),
            Place::Field { base, field } => {
                if let Some((off, _)) = self.field_layout(*base, *field) {
                    self.field_addr(*base, off);
                }
            }
            Place::Index {
                base,
                index,
                unchecked,
            } => {
                if let Some(ety) = self.array_elem_ty(*base) {
                    self.elem_addr(*base, ety, index, *unchecked);
                }
            }
            Place::Deref { ptr, .. } => self.f.local_get(&(ptr.0).to_string()),
            Place::Global(g) => self.f.global_get(&format!("g{}", g.0)),
        }
    }

    /// Retain nested refs of a value-struct *place* argument (field/index/global). Locals are
    /// handled by [`crate::Statement::ValueRetain`] so last-use call args can skip this.
    pub(super) fn emit_place_value_arg_retain(&mut self, o: &Operand) {
        if matches!(o, Operand::Copy(Place::Local(_))) {
            return;
        }
        let ty = self.operand_ty(o);
        if !self.interner.is_value_type(ty) || !self.value_has_glue(ty) {
            return;
        }
        if let Some(name) = self.value_name(ty) {
            self.emit_operand_addr(o);
            self.f.call(&vs_retain_sym(&name));
        }
    }

    /// Pushes the address of a value-struct operand.
    pub(super) fn emit_operand_addr(&mut self, o: &Operand) {
        match o {
            Operand::Copy(p) => self.emit_place_addr(p),
            Operand::Const(_) => self.f.i32_const(0),
        }
    }

    /// Byte-wise copies value struct `ty` from the `src` address to the `dst` address, then retains
    /// the destination's (now duplicated) reference fields so the copy owns its own references.
    pub(super) fn emit_value_copy(
        &mut self,
        dst: impl Fn(&mut Self),
        src: impl Fn(&mut Self),
        ty: TypeId,
        retain: bool,
    ) {
        let size = self.value_size(ty);
        dst(self);
        src(self);
        self.f.i32_const((size) as i32);
        self.f.memory_copy();
        if retain && self.value_has_glue(ty) {
            if let Some(name) = self.value_name(ty) {
                dst(self);
                self.f.call(&vs_retain_sym(&name));
            }
        }
    }

    /// Drops the value struct `ty` at the `at` address (runs `del`, releases reference fields), if it
    /// needs glue.
    pub(super) fn emit_value_drop(&mut self, at: impl Fn(&mut Self), ty: TypeId) {
        if self.value_has_glue(ty) {
            if let Some(name) = self.value_name(ty) {
                at(self);
                self.f.call(&vs_drop_sym(&name));
            }
        }
    }

    /// Constructs a value struct in place at the `dst` address: zero its bytes, then (if it has a
    /// user constructor) call `ctor(this = dst, args...)`.
    fn construct_value_new(
        &mut self,
        dst: impl Fn(&mut Self),
        ctor: Option<DefId>,
        args: &[Operand],
        ty: TypeId,
    ) {
        let size = self.value_size(ty);
        dst(self);
        self.f.i32_const(0);
        self.f.i32_const((size) as i32);
        self.f.memory_fill();
        if let Some(ctor) = ctor {
            dst(self);
            for arg in args {
                self.emit_operand(arg);
            }
            let sym = self.callee_symbol(&crate::Callee {
                def: ctor,
                args: vec![],
                ret: self.interner.void(),
                take_params: vec![],
            });
            self.f.call(&sym);
        }
    }

    /// Constructs a tuple in place at `dst`: zero the block, then store each element at its field
    /// offset (value elements are copied; references are stored and retained).
    fn construct_value_tuple(&mut self, dst: impl Fn(&mut Self), ty: TypeId, elems: &[Operand]) {
        let size = self.value_size(ty);
        dst(self);
        self.f.i32_const(0);
        self.f.i32_const((size) as i32);
        self.f.memory_fill();
        let fields: Vec<(u32, TypeId)> = self
            .layouts
            .get(ty)
            .map(|l| l.fields.iter().map(|f| (f.offset, f.ty)).collect())
            .unwrap_or_default();
        for (i, arg) in elems.iter().enumerate() {
            let Some(&(off, fty)) = fields.get(i) else {
                continue;
            };
            let field_addr = |s: &mut Self| {
                dst(s);
                if off > 0 {
                    s.f.i32_const((off) as i32);
                    s.f.i32_add();
                }
            };
            if self.interner.is_value_type(fty) {
                let arg = arg.clone();
                self.emit_value_copy(field_addr, |s| s.emit_operand_addr(&arg), fty, true);
            } else {
                field_addr(self);
                self.emit_operand(arg);
                self.f.store(self.store_kind(fty), 0);
                self.retain_container_value(fty, arg);
            }
        }
    }

    /// Emits a direct call to a value-struct-returning function using the sret ABI: the destination
    /// address (produced by `dst`) is passed as the hidden leading argument, then the real arguments.
    fn emit_value_sret_call(
        &mut self,
        dst: impl Fn(&mut Self),
        callee: &crate::Callee,
        args: &[Operand],
    ) {
        dst(self);
        self.emit_call_args(callee, args);
        self.f.call(&self.callee_symbol(callee));
    }

    /// Constructs a value union in place at the `dst` address: zero the block, write the variant
    /// discriminant at offset 0, then store each payload argument at its variant field offset (a
    /// value payload is copied inline; a reference payload is stored and retained).
    fn construct_value_union(
        &mut self,
        dst: impl Fn(&mut Self),
        ty: TypeId,
        variant: usize,
        args: &[Operand],
    ) {
        let size = self.value_size(ty);
        dst(self);
        self.f.i32_const(0);
        self.f.i32_const((size) as i32);
        self.f.memory_fill();
        dst(self);
        self.f.i32_const((variant) as i32);
        self.f.store(StoreKind::I32, 0);
        let fields: Vec<(u32, TypeId)> = self
            .layouts
            .union(ty)
            .and_then(|u| {
                u.variants
                    .iter()
                    .find(|v| v.discriminant as usize == variant)
                    .map(|v| v.fields.iter().map(|f| (f.offset, f.ty)).collect())
            })
            .unwrap_or_default();
        for (i, arg) in args.iter().enumerate() {
            let Some(&(off, fty)) = fields.get(i) else {
                continue;
            };
            let field_addr = |s: &mut Self| {
                dst(s);
                if off > 0 {
                    s.f.i32_const((off) as i32);
                    s.f.i32_add();
                }
            };
            if self.interner.is_value_type(fty) {
                let arg = arg.clone();
                self.emit_value_copy(field_addr, |s| s.emit_operand_addr(&arg), fty, true);
            } else {
                field_addr(self);
                self.emit_operand(arg);
                self.f.store(self.store_kind(fty), 0);
                self.retain_container_value(fty, arg);
            }
        }
    }

    /// Stores a value struct or value union produced by `rvalue` into the destination at the `dst`
    /// address (a local slot, a container field/element, or a union payload): the old contents are
    /// dropped, then the new value is constructed / sret-called / copied in place.
    pub(super) fn emit_value_store(
        &mut self,
        dst: impl Fn(&mut Self),
        ty: TypeId,
        rvalue: &Rvalue,
        copy_retain: bool,
    ) {
        self.emit_value_drop(&dst, ty);
        match rvalue {
            Rvalue::New {
                ctor,
                args,
                ty: nty,
                ..
            } => self.construct_value_new(&dst, *ctor, args, *nty),
            Rvalue::Tuple { ty: tty, elems } => self.construct_value_tuple(&dst, *tty, elems),
            Rvalue::UnionNew {
                ty: uty,
                variant,
                args,
                ..
            } => self.construct_value_union(&dst, *uty, *variant, args),
            Rvalue::Call { callee, args } => {
                if !self.try_emit_simd_call(callee, args, Some(&dst), None) {
                    self.emit_value_sret_call(&dst, callee, args);
                }
            }
            Rvalue::IndirectCall { target, sig, args } => {
                self.emit_indirect_sret_call(&dst, target, *sig, args)
            }
            Rvalue::InterfaceCall {
                receiver,
                iface_id,
                method_slot,
                sig,
                args,
                ..
            } => self.emit_interface_sret_call(&dst, receiver, *iface_id, *method_slot, *sig, args),
            Rvalue::Use(Operand::Copy(src)) => {
                if let Place::Local(l) = src {
                    if self.is_v128_local(*l) {
                        dst(self);
                        self.f.local_get(&(l.0).to_string());
                        self.f.store(StoreKind::V128, 0);
                        return;
                    }
                }
                let src = src.clone();
                self.emit_value_copy(&dst, |s| s.emit_place_addr(&src), ty, copy_retain);
            }
            // JS → value struct: fill `dst` in place via `$js_to_<Name>(j, dst)` (no heap alloc).
            Rvalue::Cast(o, from, to)
                if matches!(self.interner.kind(*from), TyKind::Js)
                    && self.interner.is_value_type(*to) =>
            {
                if let Some(sym) = js_marshal::cast_sym(self.interner, self.layouts, *from, *to) {
                    self.emit_operand(o);
                    dst(self);
                    self.f.call(&sym);
                } else {
                    crate::internal_error!("missing js→value-struct marshaler");
                }
            }
            other => {
                // Any other value-struct-producing rvalue (e.g. a `UnionField` payload extraction)
                // yields the *address* of an existing value; copy those bytes into the destination.
                let other = other.clone();
                self.emit_value_copy(&dst, |s| s.emit_rvalue(&other), ty, true);
            }
        }
    }

    /// Emits the scope-exit teardown of a function's shadow frame: drop each owning value local, then
    /// restore `$__sp`. A no-op for functions with no value frame.
    pub(super) fn emit_frame_teardown(&mut self) {
        for (local, _) in self.frame.teardown_slots(self.func) {
            let ty = self.func.local_ty(local);
            let l0 = local.0;
            self.emit_value_drop(|s| s.f.local_get(&(l0).to_string()), ty);
        }
        if self.frame.size > 0 {
            self.f.local_get("__saved_sp");
            self.f.global_set("__sp");
        }
    }
}
