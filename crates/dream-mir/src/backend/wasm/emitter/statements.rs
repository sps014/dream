//! `Statement` emission (assignment, print, retain/release, calls) for the WAT backend, plus the
//! place-store helpers used by assignment and by object-construction stores in `rvalue.rs`: retain
//! on store (`retain_container_value`), deferred release of an overwritten reference
//! (`stash_old_ref`/`release_stash`), and debug-info local spilling. Split out of `emitter.rs`;
//! these are methods on the parent's private `Emitter`.

use super::*;

/// True if `rvalue` is `Rvalue::ArrayRealloc { array, .. }` whose `array` operand reads exactly the
/// place being assigned to — the `p = Buffer.realloc<T>(p, n)` growth idiom (`List<T>.grow`,
/// `Pointer<T>.realloc`). Only `Field`/`Local`/`Global` bases are recognized (an `Index` base is
/// never treated as matching, conservatively, since two `Index` places can alias through a runtime
/// index the compiler cannot compare statically). See
/// [`Emitter::emit_place_store_no_release_old`] for why this needs special-casing.
fn realloc_self_store(place: &Place, rvalue: &Rvalue) -> bool {
    let Rvalue::ArrayRealloc { array, .. } = rvalue else {
        return false;
    };
    let Operand::Copy(src) = array else {
        return false;
    };
    match (place, src) {
        (
            Place::Field {
                base: b1,
                field: f1,
            },
            Place::Field {
                base: b2,
                field: f2,
            },
        ) => b1 == b2 && f1 == f2,
        (Place::Local(l1), Place::Local(l2)) => l1 == l2,
        (Place::Global(g1), Place::Global(g2)) => g1 == g2,
        _ => false,
    }
}

impl Emitter<'_> {
    pub(super) fn emit_stmt(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Assign(place, rvalue) => self.emit_assign(place, rvalue),
            Statement::Retain(o) => {
                let ty = self.operand_ty(o);
                self.emit_operand(o);
                self.f.call(retain_call(self.interner, ty));
            }
            Statement::Release(o) => {
                // Deep release by the operand's declared type: structs/unions/reference arrays run
                // their generated `$release_<...>`; `js` handles call `$js_release`; other
                // references fall back to the generic/tag-dispatched runtime.
                let ty = self.operand_ty(o);
                let call = if self.interner.is_rc_tracked(ty) {
                    release_call(self.interner, self.layouts, ty)
                } else {
                    "$release_generic".to_string()
                };
                self.emit_operand(o);
                self.f.call(&call);
            }
            Statement::ReleaseUnique(o) => {
                let ty = self.operand_ty(o);
                let call = if self.interner.is_rc_tracked(ty) {
                    destroy_call(self.interner, self.layouts, ty)
                } else {
                    "$release_generic".to_string()
                };
                self.emit_operand(o);
                self.f.call(&call);
            }
            Statement::Panic(msg) => {
                self.emit_operand(msg);
                self.f.call("dream_panic");
            }
            Statement::Call { callee, args } => {
                if !self.try_emit_simd_call(callee, args, None::<fn(&mut Self)>, None) {
                    self.emit_call_args(callee, args);
                    self.f.call(&self.callee_symbol(callee));
                    if !matches!(self.interner.kind(callee.ret), TyKind::Void) {
                        self.f.drop_();
                    }
                }
            }
            Statement::JsCall {
                callee,
                target,
                via,
                method,
                args,
            } => {
                self.emit_js_call(callee, target, via.as_ref(), method.as_ref(), args);
                if !matches!(self.interner.kind(callee.ret), TyKind::Void) {
                    self.f.drop_();
                }
            }
            Statement::InterfaceCall {
                receiver,
                iface_id,
                method_slot,
                sig,
                args,
            } => {
                self.emit_interface_call(receiver, *iface_id, *method_slot, *sig, args);
                let ret = match self.interner.kind(*sig) {
                    TyKind::Func(_, r) => Some(*r),
                    _ => None,
                };
                let drops = ret
                    .map(|r| !matches!(self.interner.kind(r), TyKind::Void))
                    .unwrap_or(false);
                if drops {
                    self.f.drop_();
                }
            }
            Statement::IndirectCall { target, sig, args } => {
                if self.emit_indirect_call(target, *sig, args).is_some() {
                    self.f.drop_();
                }
            }
            Statement::Print { arg, ty, newline } => {
                // Push the value, then print it. `int`/`char`/`string` go straight to a host import;
                // every other scalar is first rendered with its in-wasm `*_to_string` and printed as a
                // string. `println` appends a trailing newline (`\n` = 10) via `$print_char`.
                self.emit_operand(arg);
                match self.interner.kind(*ty) {
                    TyKind::Prim(PrimTy::Int) => self.f.call("print_int"),
                    TyKind::Prim(PrimTy::Char) => self.f.call("print_char"),
                    TyKind::Prim(PrimTy::String) => self.f.call("print_string"),
                    // Int/Char/String are handled above; every other primitive renders through its
                    // shared `$*_to_string` formatter and prints as a string.
                    TyKind::Prim(prim) => match prim_info(*prim).to_string {
                        Some(to_string) => {
                            self.f.call(to_string);
                            self.f.call("print_string");
                        }
                        None => self.f.call("print_int"),
                    },
                    // Enums are `i32` values at runtime; print their numeric value.
                    TyKind::Enum(_) => self.f.call("print_int"),
                    // Arrays aren't self-describing at runtime (the header only says `TAG_ARRAY`), so
                    // the element-typed `to_string` is chosen statically here, then printed.
                    TyKind::Array(elem) => {
                        self.f.call(&array_to_string_sym(*elem));
                        self.f.call("print_string");
                    }
                    // A value struct/union has no heap tag header, so it is rendered by its concrete
                    // `$<Type>_to_string` (chosen statically from the operand's type) and printed.
                    _ if self.interner.is_value_type(*ty) => {
                        if let Some(name) = self.value_name(*ty) {
                            self.f.call(&format!("{}_to_string", name));
                            self.f.call("print_string");
                        } else {
                            self.f.call("print_object");
                        }
                    }
                    // Reference structs, unions, and `object` render through the tag-dispatching
                    // `$print_object` (which routes to each type's `to_string`).
                    _ => self.f.call("print_object"),
                }
                if *newline {
                    self.f.i32_const(10);
                    self.f.call("print_char");
                }
            }
            Statement::Nop => {}
            Statement::DebugLine(line) => self.emit_debug_line(*line),
            // Emits nothing: just remembers the line so a following automatic runtime check can
            // attribute its panic message to it (see `Emitter::current_line`/`Emitter::emit_panic`).
            Statement::SourceLine(line) => self.current_line = *line,
            Statement::SimdV128 {
                lane,
                op,
                dest,
                lhs,
                rhs,
                index,
                splat_rhs,
                ptr_addr,
            } => {
                if *ptr_addr {
                    self.emit_operand(dest);
                } else {
                    self.emit_v128_array_addr(dest, index, *lane);
                }
                if *ptr_addr {
                    self.emit_operand(lhs);
                } else {
                    self.emit_v128_array_addr(lhs, index, *lane);
                }
                self.f.load(LoadKind::V128, 0);
                if let Some(s) = splat_rhs {
                    self.emit_operand(s);
                    self.emit_simd_splat(*lane);
                } else if *ptr_addr {
                    self.emit_operand(rhs);
                    self.f.load(LoadKind::V128, 0);
                } else {
                    self.emit_v128_array_addr(rhs, index, *lane);
                    self.f.load(LoadKind::V128, 0);
                }
                self.emit_simd_binop(*lane, *op);
                self.f.store(StoreKind::V128, 0);
            }
            Statement::ForceFree(o) => {
                self.emit_operand(o);
                self.f.call("free");
            }
            Statement::ArrayElemsCopy {
                elem_ty,
                dst,
                dst_off,
                src,
                src_off,
                count,
            } => {
                // `memory.copy(dst+4+dst_off*esize, src+4+src_off*esize, count*esize)`.
                let (esize, _) = scalar_size(self.interner, *elem_ty);
                self.emit_operand(dst);
                self.f.local_set("__obj");
                self.emit_operand(src);
                self.f.local_set("__src");
                self.emit_operand(count);
                self.f.local_set("__len");
                self.f.local_get("__obj");
                self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                self.f.i32_add();
                self.emit_operand(dst_off);
                self.f.i32_const((esize) as i32);
                self.f.i32_mul();
                self.f.i32_add();
                self.f.local_get("__src");
                self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                self.f.i32_add();
                self.emit_operand(src_off);
                self.f.i32_const((esize) as i32);
                self.f.i32_mul();
                self.f.i32_add();
                self.f.local_get("__len");
                self.f.i32_const((esize) as i32);
                self.f.i32_mul();
                self.f.memory_copy();
            }
            Statement::ArrayElemsFill {
                elem_ty,
                dst,
                dst_off,
                count,
            } => {
                let (esize, _) = scalar_size(self.interner, *elem_ty);
                self.emit_operand(dst);
                self.f.local_set("__obj");
                self.emit_operand(count);
                self.f.local_set("__len");
                self.f.local_get("__obj");
                self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                self.f.i32_add();
                self.emit_operand(dst_off);
                self.f.i32_const((esize) as i32);
                self.f.i32_mul();
                self.f.i32_add();
                self.f.i32_const(0);
                self.f.local_get("__len");
                self.f.i32_const((esize) as i32);
                self.f.i32_mul();
                self.f.memory_fill();
            }
            Statement::LockAcquire(o) => {
                self.emit_lock_addr(o);
                self.f.call("__lock_acquire");
            }
            Statement::LockRelease(o) => {
                self.emit_lock_addr(o);
                self.f.call("__lock_release");
            }
            Statement::DeferEnter => {
                self.f.call("dream_defer_enter");
            }
            Statement::DeferLeave(o) => {
                self.emit_operand(o);
                self.f.call("dream_defer_leave");
            }
            Statement::ValueDrop(local) => {
                if self.is_v128_local(*local) {
                    return;
                }
                let ty = self.func.local_ty(*local);
                debug_assert!(
                    self.interner.is_value_type(ty),
                    "ValueDrop on non-value local"
                );
                let l0 = local.0;
                self.emit_value_drop(|s| s.f.local_get(&(l0).to_string()), ty);
                // Null RC fields so loop re-entry into an inlined region (value_store's pre-drop)
                // only releases null. Nested inlines must not emit a second `ValueDrop` for the
                // same local (the inliner skips already-`manual_drop` locals when collecting).
                if let Some(layout) = self.layouts.get(ty) {
                    for f in &layout.fields {
                        if self.interner.is_rc_tracked(f.ty) {
                            self.f.local_get(&(l0).to_string());
                            if f.offset > 0 {
                                self.f.i32_const((f.offset) as i32);
                                self.f.i32_add();
                            }
                            self.f.i32_const(0);
                            self.f.store(StoreKind::I32, 0);
                        }
                    }
                }
            }
            Statement::ValueRetain(local) => {
                let ty = self.func.local_ty(*local);
                debug_assert!(
                    self.interner.is_value_type(ty),
                    "ValueRetain on non-value local"
                );
                if self.value_has_glue(ty) {
                    if let Some(name) = self.value_name(ty) {
                        self.f.local_get(&(local.0).to_string());
                        self.f.call(&vs_retain_sym(&name));
                    }
                }
            }
            Statement::ValueKill(local) => {
                if self.is_v128_local(*local) {
                    return;
                }
                let ty = self.func.local_ty(*local);
                debug_assert!(
                    self.interner.is_value_type(ty),
                    "ValueKill on non-value local"
                );
                let size = self.value_size(ty);
                if size > 0 {
                    self.f.local_get(&(local.0).to_string());
                    self.f.i32_const(0);
                    self.f.i32_const((size) as i32);
                    self.f.memory_fill();
                }
            }
        }
    }

    /// Pushes the address of `o`'s (an `@shared class`-typed pointer operand) embedded lock word:
    /// `obj_ptr + layout.size`, i.e. the first word past the object's last field — see
    /// `src/mir/abi.rs`'s `@shared class` header-extension note and the `is_shared` branch of
    /// `Rvalue::New` emission, which reserves and zeroes exactly this word.
    fn emit_lock_addr(&mut self, o: &Operand) {
        let ty = self.operand_ty(o);
        let size = self.layouts.get(ty).map(|l| l.size).unwrap_or(0);
        self.emit_operand(o);
        if size > 0 {
            self.f.i32_const((size) as i32);
            self.f.i32_add();
        }
    }

    /// Emits the debug-info instrumentation for a source-line boundary: spill every named local into
    /// the exported `$__dbg_v{k}` global pool (so the host debugger can read live values), then call
    /// the `dream_debug.line` host hook with `(file_id, line)`. A no-op unless debug-info is on for
    /// this function.
    fn emit_debug_line(&mut self, line: u32) {
        let Some(dbg) = self.debug_fn else {
            return;
        };
        let file_id = dbg.file;
        // Snapshot the spill descriptors so we can borrow `self` mutably while emitting.
        let vars: Vec<(u32, u32, crate::backend::wasm::debug_map::SpillKind)> = dbg
            .vars
            .iter()
            .map(|v| (v.local, v.global, v.spill))
            .collect();
        for (local, global, kind) in vars {
            self.emit_var_spill(local, global, kind);
        }
        self.f.i32_const(file_id as i32);
        self.f.i32_const(line as i32);
        self.f.call("__dbg_line");
    }

    /// Spills one named local into its `i64` pool global, widening/reinterpreting to preserve the
    /// exact bits so the host can decode the value back using the variable's declared kind.
    fn emit_var_spill(
        &mut self,
        local: u32,
        global: u32,
        kind: crate::backend::wasm::debug_map::SpillKind,
    ) {
        use crate::backend::wasm::debug_map::SpillKind as K;
        self.f.local_get(&local.to_string());
        match kind {
            K::I64 => {}
            K::F64 => self.f.i64_reinterpret_f64(),
            K::F32 => {
                self.f.i32_reinterpret_f32();
                self.f.i64_extend_i32_u();
            }
            K::I32 => self.f.i64_extend_i32_u(),
        }
        self.f.global_set(&format!("__dbg_v{global}"));
    }

    fn emit_assign(&mut self, place: &Place, rvalue: &Rvalue) {
        match place {
            Place::Local(l) => {
                let ty = self.func.local_ty(*l);
                if self.is_v128_local(*l) {
                    match rvalue {
                        Rvalue::Call { callee, args } => {
                            if !self.try_emit_simd_call(
                                callee,
                                args,
                                None::<fn(&mut Self)>,
                                Some(l.0),
                            ) {
                                self.emit_v128_sret_into_local(l.0, callee, args);
                            }
                        }
                        Rvalue::Use(Operand::Copy(Place::Local(src)))
                            if self.is_v128_local(*src) =>
                        {
                            self.f.local_get(&(src.0).to_string());
                            self.f.local_set(&(l.0).to_string());
                        }
                        Rvalue::Use(o) => {
                            self.emit_v128_operand(o);
                            self.f.local_set(&(l.0).to_string());
                        }
                        Rvalue::New { args, .. } => {
                            if let Some(v) = args.first() {
                                self.emit_operand(v);
                                let lane = match self.interner.kind(ty) {
                                    dream_types::TyKind::Struct(_, ts) => {
                                        ts.first().and_then(|e| {
                                            crate::SimdLane::from_elem(self.interner, *e)
                                        })
                                    }
                                    _ => None,
                                }
                                .unwrap_or(crate::SimdLane::I32);
                                self.emit_simd_splat(lane);
                                self.f.local_set(&(l.0).to_string());
                            }
                        }
                        other => crate::internal_error!(
                            "unsupported rvalue for Vector v128 local: {:?}",
                            other
                        ),
                    }
                    return;
                }
                if self.interner.is_value_type(ty) {
                    let l0 = l.0;
                    match self.frame.kind(*l) {
                        Some(ValueLocalKind::Owning) => {
                            let copy_retain =
                                !matches!(rvalue, Rvalue::Use(Operand::Copy(Place::Local(_))));
                            self.emit_value_store(
                                |s| s.f.local_get(&(l0).to_string()),
                                ty,
                                rvalue,
                                copy_retain,
                            );
                        }
                        // A borrow/param value local just holds an address: rebind it to the source
                        // value's address (no copy, no drop).
                        _ => {
                            match rvalue {
                                Rvalue::Use(o) => self.emit_operand_addr(o),
                                _ => self.emit_rvalue(rvalue),
                            }
                            self.f.local_set(&(l0).to_string());
                        }
                    }
                    return;
                }
                self.emit_rvalue(rvalue);
                self.f.local_set(&(l.0).to_string());
            }
            Place::Global(g) => {
                if let Some(&ty) = self.global_tys.get(&g.0) {
                    if self.interner.is_value_type(ty) {
                        let g0 = g.0;
                        self.emit_value_store(
                            |s| s.f.global_get(&format!("g{}", g0)),
                            ty,
                            rvalue,
                            true,
                        );
                        return;
                    }
                }
                self.emit_rvalue(rvalue);
                self.f.global_set(&format!("g{}", g.0));
            }
            Place::Field { base, field } => {
                if self.is_v128_local(*base) {
                    let lane = self
                        .layouts
                        .get(self.func.local_ty(*base))
                        .and_then(|l| l.fields.get(*field))
                        .map(|f| f.offset / 4)
                        .unwrap_or(0);
                    self.f.local_get(&(base.0).to_string());
                    self.emit_rvalue(rvalue);
                    self.f.replace_lane(ReplaceLane::I32x4, lane as u8);
                    self.f.local_set(&(base.0).to_string());
                    return;
                }
                if let Some(f) = self.field_layout_full(*base, *field) {
                    let (off, fty, is_weak, is_unowned) = (f.offset, f.ty, f.is_weak, f.is_unowned);
                    let b = *base;
                    if is_weak {
                        self.emit_weak_field_store(b, off, fty, rvalue);
                    } else if is_unowned {
                        self.emit_unowned_field_store(b, off, fty, rvalue);
                    } else if realloc_self_store(place, rvalue) {
                        self.emit_place_store_no_release_old(
                            fty,
                            move |s| s.field_addr(b, off),
                            rvalue,
                        );
                    } else {
                        self.emit_place_store(fty, move |s| s.field_addr(b, off), rvalue);
                    }
                } else {
                    crate::internal_error!(
                        "missing field layout for store (base {:?}, field {})",
                        base,
                        field
                    );
                }
            }
            Place::Index {
                base,
                index,
                unchecked,
            } => {
                if let Some(ety) = self.array_elem_ty(*base) {
                    let b = *base;
                    let idx = index.clone();
                    let uc = *unchecked;
                    self.emit_place_store(ety, move |s| s.elem_addr(b, ety, &idx, uc), rvalue);
                } else {
                    crate::internal_error!(
                        "missing array element type for store (base {:?})",
                        base
                    );
                }
            }
            Place::Deref { ptr, elem_ty } => {
                let p = *ptr;
                let ety = *elem_ty;
                self.emit_place_store(ety, move |s| s.f.local_get(&(p.0).to_string()), rvalue);
            }
        }
    }

    pub(super) fn emit_v128_array_addr(
        &mut self,
        arr: &Operand,
        index: &Operand,
        lane: crate::SimdLane,
    ) {
        self.emit_operand(arr);
        self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
        self.f.i32_add();
        self.emit_operand(index);
        let sh = lane.shift();
        if sh == 0 {
            self.f.i32_add();
        } else {
            self.f.i32_const((sh) as i32);
            self.f.i32_shl();
            self.f.i32_add();
        }
    }

    /// Stores `rvalue` into a memory place of type `ty` whose address is produced by `addr`. Shared by
    /// field and array-element assignment, which differ only in how the slot address is computed. A
    /// value(`struct`) slot is copied in place (`emit_value_store`); a reference/scalar slot stashes
    /// the previous occupant, stores the new value with the slot's width, retains a stored borrowed
    /// reference, then releases the stashed old reference (deferred so self-referential writes stay
    /// sound).
    ///
    /// For a *borrowed* reference store (`Use(Copy(_))` / `Use(Const::Str(_))`), identity-elides when
    /// the new pointer equals the old occupant: stash old → `$__rel`, new → `$__src`, and only
    /// store+retain+release when `$__rel != $__src`.
    fn emit_place_store(&mut self, ty: TypeId, addr: impl Fn(&mut Self), rvalue: &Rvalue) {
        if self.interner.is_value_type(ty) {
            self.emit_value_store(addr, ty, rvalue, true);
            return;
        }
        let take_transfer = matches!(
            rvalue,
            Rvalue::Use(Operand::Copy(Place::Local(l)))
                if self.func.locals.get(l.0 as usize).is_some_and(|d| d.is_take)
        );
        let borrowed_ref = self.interner.is_rc_tracked(ty)
            && matches!(
                rvalue,
                Rvalue::Use(Operand::Copy(_)) | Rvalue::Use(Operand::Const(Const::Str(_)))
            )
            && !take_transfer;
        if borrowed_ref {
            addr(self);
            self.f.load(LoadKind::I32, 0);
            self.f.local_set("__rel");
            self.emit_rvalue(rvalue);
            self.f.local_set("__src");
            self.f.local_get("__rel");
            self.f.local_get("__src");
            self.f.i32_ne();
            self.f.if_();
            addr(self);
            self.f.local_get("__src");
            self.f.store(self.store_kind(ty), 0);
            self.f.local_get("__src");
            self.f.call(retain_call(self.interner, ty));
            let release = release_call(self.interner, self.layouts, ty);
            self.f.local_get("__rel");
            self.f.call(&release);
            self.f.end();
            return;
        }
        let stash = self.stash_old_ref(ty, &addr);
        addr(self);
        self.emit_rvalue(rvalue);
        self.f.store(self.store_kind(ty), 0);
        self.retain_stored_rvalue(ty, rvalue);
        self.release_stash(ty, stash);
    }

    /// Like [`Self::emit_place_store`], but skips the stash/release of the slot's previous
    /// occupant. Used exactly when `rvalue` is `Rvalue::ArrayRealloc` reading the *same* place being
    /// stored to (see [`realloc_self_store`]): `$realloc` has already consumed the old block itself
    /// (freed it outright if the block moved, or reused it in place otherwise), so the ordinary
    /// release-old-occupant step would double-free/decrement a block the allocator may have already
    /// handed to someone else.
    fn emit_place_store_no_release_old(
        &mut self,
        ty: TypeId,
        addr: impl Fn(&mut Self),
        rvalue: &Rvalue,
    ) {
        addr(self);
        self.emit_rvalue(rvalue);
        self.f.store(self.store_kind(ty), 0);
    }

    /// Stores into a `weak` field (`ty` is `Option<T>` for a class `T`; see `docs/language/memory.md`
    /// and `src/mir/runtime/weak.wat`). A `weak` field never holds a strong reference to its
    /// payload, so it cannot be stored through the ordinary place-store path (which would retain the
    /// payload on store and deep-release it on overwrite/teardown, just like a normal strong field).
    /// Instead: the RHS is evaluated and fully strongly owned as usual (so any temporary is correctly
    /// retained/released by the surrounding rvalue machinery), but only its `(discriminant, payload)`
    /// bits are copied into a *fresh, privately managed* weak-box (never retaining the payload); that
    /// box's address is registered as watching the payload (if `Some`) and stored into the field —
    /// only *then* is the old occupant's box unregistered and freed directly (deferred exactly like
    /// the ordinary stash/release rule, so a self-referential `n.parent = f(n.parent)` stays sound);
    /// finally the RHS temporary itself is released (if owned), since the field never took ownership
    /// of it.
    fn emit_weak_field_store(
        &mut self,
        base: crate::Local,
        offset: u32,
        option_ty: TypeId,
        rvalue: &Rvalue,
    ) {
        let Some(u) = self.layouts.union(option_ty) else {
            crate::internal_error!("weak field's type {:?} has no union layout", option_ty);
        };
        let some_disc = u.variant("Some").map(|v| v.discriminant).unwrap_or(0);
        let none_disc = u.variant("None").map(|v| v.discriminant).unwrap_or(1);
        let payload_off = u
            .variant("Some")
            .and_then(|v| v.fields.first())
            .map(|fl| fl.offset)
            .unwrap_or(4);
        let box_size = u.size;

        // Evaluate the RHS normally (a fully-owned-or-borrowed `Option<T>` per the usual rules) and
        // peek at its bits.
        self.emit_rvalue(rvalue);
        self.f.local_set("__wsrc");

        // Stash the old occupant's box pointer (read-only; freed only after the new box is in place).
        self.field_addr(base, offset);
        self.f.load(LoadKind::I32, 0);
        self.f.local_set("__rel");

        // Allocate the fresh private box and copy the (discriminant, payload) bits into it, without
        // retaining the payload.
        self.f.i32_const((box_size) as i32);
        self.f.i32_const(0);
        self.f.call("malloc");
        self.f.local_set("__wbox");
        self.f.local_get("__wbox");
        self.f.local_get("__wsrc");
        self.f.load(LoadKind::I32, 0);
        self.f.store(StoreKind::I32, 0);
        self.f.local_get("__wbox");
        self.f.i32_const(payload_off as i32);
        self.f.i32_add();
        self.f.local_get("__wsrc");
        self.f.i32_const(payload_off as i32);
        self.f.i32_add();
        self.f.load(LoadKind::I32, 0);
        self.f.store(StoreKind::I32, 0);

        // Register the new box as watching its payload, if it's live.
        self.f.local_get("__wsrc");
        self.f.load(LoadKind::I32, 0);
        self.f.i32_const(some_disc);
        self.f.i32_eq();
        self.f.if_();
        self.f.local_get("__wsrc");
        self.f.i32_const(payload_off as i32);
        self.f.i32_add();
        self.f.load(LoadKind::I32, 0);
        self.f.local_get("__wbox");
        self.f.i32_const(0);
        self.f.i32_const(none_disc);
        self.f.call("weak_register");
        self.f.end();

        // Store the new box into the field.
        self.field_addr(base, offset);
        self.f.local_get("__wbox");
        self.f.store(StoreKind::I32, 0);

        // Now that the new box is safely in place, tear down the old one: unregister it (if it
        // currently watches a live referent) and free it directly — never through
        // `$release_<Option_...>`, since the box was never a strong owner of its payload.
        self.f.local_get("__rel");
        self.f.if_();
        self.f.local_get("__rel");
        self.f.load(LoadKind::I32, 0);
        self.f.i32_const(some_disc);
        self.f.i32_eq();
        self.f.if_();
        self.f.local_get("__rel");
        self.f.i32_const(payload_off as i32);
        self.f.i32_add();
        self.f.load(LoadKind::I32, 0);
        self.f.local_get("__rel");
        self.f.call("weak_unregister");
        self.f.end();
        self.f.local_get("__rel");
        self.f.call("free");
        self.f.end();

        // The field never owns the RHS wrapper itself; release it if it was a fresh, owned value
        // (a borrowed copy is left untouched, matching the ordinary store rule).
        if !matches!(rvalue, Rvalue::Use(Operand::Copy(_))) {
            self.f.local_get("__wsrc");
            let call = release_call(self.interner, self.layouts, option_ty);
            self.f.call(&call);
        }
    }

    /// Stores into an `unowned` field (`ty` is a plain class type; see `docs/language/memory.md` and
    /// `src/mir/runtime/weak.wat`). An `unowned` field holds the referent's raw pointer directly (no
    /// wrapper), never retains it on store, and is poisoned to `0` by `$weak_clear_all` if the
    /// referent is freed while still watched — a later read then traps (see
    /// `Emitter::emit_unowned_read_check`).
    fn emit_unowned_field_store(
        &mut self,
        base: crate::Local,
        offset: u32,
        field_ty: TypeId,
        rvalue: &Rvalue,
    ) {
        // Unregister the old occupant, if any (no box to free — the field itself was the slot).
        self.field_addr(base, offset);
        self.f.load(LoadKind::I32, 0);
        self.f.local_set("__wsrc");
        self.f.local_get("__wsrc");
        self.f.if_();
        self.f.local_get("__wsrc");
        self.field_addr(base, offset);
        self.f.call("weak_unregister");
        self.f.end();

        // Evaluate the RHS, store it directly (no retain), and register it as a new watcher of its
        // referent *before* possibly releasing it below — so an RHS that was its own referent's only
        // owner is correctly poisoned back to `0` right away, rather than left dangling.
        self.emit_rvalue(rvalue);
        self.f.local_set("__wbox");
        self.field_addr(base, offset);
        self.f.local_get("__wbox");
        self.f.store(StoreKind::I32, 0);
        self.f.local_get("__wbox");
        self.f.if_();
        self.f.local_get("__wbox");
        self.field_addr(base, offset);
        self.f.i32_const(1);
        self.f.i32_const(0);
        self.f.call("weak_register");
        self.f.end();

        // The field never takes ownership of the RHS; give back its `+1` if it was a fresh, owned
        // value (a borrowed copy is left untouched).
        if !matches!(rvalue, Rvalue::Use(Operand::Copy(_))) {
            self.f.local_get("__wbox");
            let call = release_call(self.interner, self.layouts, field_ty);
            self.f.call(&call);
        }
    }

    /// Stores `value` into the object under construction (`$__obj + offset`) with the field/element
    /// width. Used by `New`/`ArrayLit` initialization. A *borrowed* reference (a copy of an existing
    /// place) is retained, since the container becomes a new owner; an owned producer is not
    /// materialized here (lowering routes those through a temporary that is itself released at scope
    /// exit), so retaining a copied operand is the sound, uniform rule.
    pub(super) fn store_at_obj(&mut self, offset: u32, value_ty: TypeId, value: &Operand) {
        // A value struct stored into a freshly-allocated container is copied inline (byte-wise + a
        // retain of its reference fields); the block was just zeroed, so there is no old value to
        // drop.
        if self.interner.is_value_type(value_ty) {
            let value = value.clone();
            self.emit_value_copy(
                |s| {
                    s.f.local_get("__obj");
                    if offset > 0 {
                        s.f.i32_const((offset) as i32);
                        s.f.i32_add();
                    }
                },
                |s| s.emit_operand_addr(&value),
                value_ty,
                true,
            );
            return;
        }
        self.f.local_get("__obj");
        if offset > 0 {
            self.f.i32_const((offset) as i32);
            self.f.i32_add();
        }
        self.emit_operand(value);
        self.f.store(self.store_kind(value_ty), 0);
        self.retain_container_value(value_ty, value);
    }

    /// Emits a `$retain` / `$js_retain` of an RC-tracked value being stored into a container (struct
    /// field, array element, or union payload), so the container owns its own reference count. A
    /// no-op for non-tracked values and non-place operands.
    ///
    /// Last-use unique local: the container inherits the existing +1 (skip retain, null the local).
    pub(super) fn retain_container_value(&mut self, value_ty: TypeId, value: &Operand) {
        if let Some(id) =
            crate::backend::shared::unique_container_move_local(self.func, self.interner, value)
        {
            self.f.i32_const(0);
            self.f.local_set(&id.to_string());
            return;
        }
        let borrowed = matches!(value, Operand::Copy(_) | Operand::Const(Const::Str(_)));
        if self.interner.is_rc_tracked(value_ty) && borrowed {
            self.emit_operand(value);
            self.f.call(retain_call(self.interner, value_ty));
        }
    }

    /// Before an RC-tracked field/element is overwritten, load and stash its previous occupant into
    /// the `$__rel` scratch so it can be released *after* the new value is stored (a deferred
    /// release keeps self-referential reassignments like `n.next = f(n.next)` sound). `emit_addr`
    /// pushes the slot's address. Returns `true` when a value was stashed. A no-op for non-tracked
    /// slots, and releasing a null previous value (fresh field) is a runtime no-op.
    fn stash_old_ref(&mut self, ty: TypeId, emit_addr: impl Fn(&mut Self)) -> bool {
        if !self.interner.is_rc_tracked(ty) {
            return false;
        }
        emit_addr(self);
        self.f.load(LoadKind::I32, 0);
        self.f.local_set("__rel");
        true
    }

    /// Releases the value stashed by [`Self::stash_old_ref`] (the overwritten field/element's previous
    /// occupant), if any.
    fn release_stash(&mut self, ty: TypeId, stashed: bool) {
        if !stashed {
            return;
        }
        let call = release_call(self.interner, self.layouts, ty);
        self.f.local_get("__rel");
        self.f.call(&call);
    }

    /// Like [`Self::retain_container_value`] but for a field/element written from an rvalue: a
    /// *borrowed* value (`Use(Copy(place))`) is retained, while an owned producer (call/new/array
    /// literal result) transfers its `+1` into the container and is left as-is.
    fn retain_stored_rvalue(&mut self, ty: TypeId, rvalue: &Rvalue) {
        if let Rvalue::Use(value) | Rvalue::Cast(value, _, _) = rvalue {
            self.retain_container_value(ty, value);
        }
    }

    /// Writes a zero of `field_ty`'s width into the object under construction (`$__obj + offset`).
    /// Used to clear a struct before a user constructor runs (reused heap blocks are not zeroed).
    pub(super) fn zero_at_obj(&mut self, offset: u32, field_ty: TypeId) {
        self.f.local_get("__obj");
        if offset > 0 {
            self.f.i32_const((offset) as i32);
            self.f.i32_add();
        }
        match self.store_kind(field_ty) {
            StoreKind::F64 => self.f.f64_const(0.0),
            StoreKind::F32 => self.f.f32_const(0.0),
            StoreKind::I64 => self.f.i64_const(0),
            _ => self.f.i32_const(0),
        }
        self.f.store(self.store_kind(field_ty), 0);
    }
}
