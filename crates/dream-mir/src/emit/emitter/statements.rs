//! `Statement` emission (assignment, print, calls) for the WAT backend, plus place-store helpers
//! and debug-info local spilling.

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
            Statement::Assign(place, rvalue) => self.emit_assign(place, rvalue, true),
            Statement::AssignNoDrop(place, rvalue) => self.emit_assign(place, rvalue, false),
            Statement::Panic(msg) => {
                self.emit_operand(msg);
                self.line("     (call $dream_panic)");
            }
            Statement::Call { callee, args } => {
                let sym = self.callee_symbol(callee);
                self.emit_call_args(callee, args);
                self.line(&format!("     (call ${})", sym));
                if !matches!(self.interner.kind(callee.ret), TyKind::Void) {
                    self.emit_discard(callee.ret);
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
                    self.emit_discard(callee.ret);
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
                    self.emit_discard(ret.unwrap());
                }
            }
            Statement::IndirectCall { target, sig, args } => {
                let has_val = self.emit_indirect_call(target, *sig, args).is_some();
                if has_val {
                    let ret = match self.interner.kind(*sig) {
                        TyKind::Func(_, r) => *r,
                        _ => self.interner.void(),
                    };
                    self.emit_discard(ret);
                }
            }
            Statement::Print { arg, ty, newline } => {
                // Push the value, then print it. `int`/`char`/`string` go straight to a host import;
                // every other scalar is first rendered with its in-wasm `*_to_string` and printed as a
                // string. `println` appends a trailing newline (`\n` = 10) via `$print_char`.
                self.emit_operand(arg);
                match self.interner.kind(*ty) {
                    TyKind::Prim(PrimTy::Int) => self.line("     (call $print_int)"),
                    TyKind::Prim(PrimTy::Char) => self.line("     (call $print_char)"),
                    TyKind::Prim(PrimTy::String) => self.line("     (call $print_string)"),
                    // Int/Char/String are handled above; every other primitive renders through its
                    // shared `$*_to_string` formatter and prints as a string.
                    TyKind::Prim(prim) => match prim_info(*prim).to_string {
                        Some(to_string) => {
                            self.line(&format!("     (call {})", to_string));
                            self.emit_print_temp_string();
                        }
                        None => self.line("     (call $print_int)"),
                    },
                    // Enums are `i32` values at runtime; print their numeric value.
                    TyKind::Enum(_) => self.line("     (call $print_int)"),
                    // Arrays aren't self-describing at runtime (the header only says `TAG_ARRAY`), so
                    // the element-typed `to_string` is chosen statically here, then printed.
                    TyKind::Array(elem) => {
                        self.line(&format!("     (call {})", array_to_string_sym(*elem)));
                        self.emit_print_temp_string();
                    }
                    // A value struct/union has no heap tag header, so it is rendered by its concrete
                    // `$<Type>_to_string` (chosen statically from the operand's type) and printed.
                    _ if self.interner.is_value_type(*ty) => {
                        if let Some(name) = self.value_name(*ty) {
                            self.line(&format!("     (call ${}_to_string)", name));
                            self.emit_print_temp_string();
                        } else {
                            self.line("     (call $print_object)");
                        }
                    }
                    // Reference structs, unions, and `object` render through the tag-dispatching
                    // `$print_object` (which routes to each type's `to_string` and drops the temp).
                    _ => self.line("     (call $print_object)"),
                }
                if *newline {
                    self.line("     (i32.const 10)");
                    self.line("     (call $print_char)");
                }
            }
            Statement::Nop => {}
            Statement::DebugLine(line) => self.emit_debug_line(*line),
            // Emits nothing: just remembers the line so a following automatic runtime check can
            // attribute its panic message to it (see `Emitter::current_line`/`Emitter::emit_panic`).
            Statement::SourceLine(line) => self.current_line = *line,
            Statement::SimdF32x4 {
                op,
                dest,
                lhs,
                rhs,
                index,
            } => {
                let simd_op = match op {
                    BinOp::Add => "f32x4.add",
                    BinOp::Sub => "f32x4.sub",
                    BinOp::Mul => "f32x4.mul",
                    _ => "f32x4.add",
                };
                self.emit_f32x4_addr(dest, index);
                self.emit_f32x4_addr(lhs, index);
                self.line("     (v128.load)");
                self.emit_f32x4_addr(rhs, index);
                self.line("     (v128.load)");
                self.line(&format!("     ({simd_op})"));
                self.line("     (v128.store)");
            }
            Statement::ForceFree(o) => {
                self.emit_operand(o);
                self.line("     (call $dream_drop)");
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
                self.line("     (local.set $__obj) ;; dst array");
                self.emit_operand(src);
                self.line("     (local.set $__src) ;; src array");
                self.emit_operand(count);
                self.line("     (local.set $__len) ;; element count");
                self.line("     (local.get $__obj)");
                self.line("     (i32.const 4)");
                self.line("     (i32.add)");
                self.emit_operand(dst_off);
                self.line(&format!("     (i32.const {})", esize));
                self.line("     (i32.mul)");
                self.line("     (i32.add) ;; dst payload + offset");
                self.line("     (local.get $__src)");
                self.line("     (i32.const 4)");
                self.line("     (i32.add)");
                self.emit_operand(src_off);
                self.line(&format!("     (i32.const {})", esize));
                self.line("     (i32.mul)");
                self.line("     (i32.add) ;; src payload + offset");
                self.line("     (local.get $__len)");
                self.line(&format!("     (i32.const {})", esize));
                self.line("     (i32.mul) ;; byte count");
                self.line("     (memory.copy)");
            }
            Statement::LockAcquire(o) => {
                self.emit_lock_addr(o);
                self.line("     (call $__lock_acquire)");
            }
            Statement::LockRelease(o) => {
                self.emit_lock_addr(o);
                self.line("     (call $__lock_release)");
            }
            Statement::ArenaEnter(o) => {
                self.emit_operand(o);
                self.line("     (call $arena_enter)");
            }
            Statement::ArenaExit => {
                self.line("     (call $arena_exit)");
            }
            Statement::ValueDrop(local) => {
                let ty = self.func.local_ty(*local);
                debug_assert!(
                    self.interner.is_value_type(ty),
                    "ValueDrop on non-value local"
                );
                let l0 = local.0;
                self.emit_value_drop(|s| s.line(&format!("     (local.get ${})", l0)), ty);
                // Null GC-tracked fields so loop re-entry into an inlined region does not keep
                // stale root slots. Nested inlines must not emit a second `ValueDrop` for the
                // same local (the inliner skips already-`manual_drop` locals when collecting).
                if let Some(layout) = self.layouts.get(ty) {
                    for f in &layout.fields {
                        if matches!(self.interner.kind(f.ty), TyKind::Js) {
                            // `vs_drop` already unregistered; just clear the word.
                            self.line(&format!("     (local.get ${})", l0));
                            if f.offset > 0 {
                                self.line(&format!("     (i32.const {}) (i32.add)", f.offset));
                            }
                            self.line("     (i32.const 0) (i32.store)");
                        } else if self.interner.is_reference(f.ty) {
                            self.line(&format!("     (local.get ${})", l0));
                            if f.offset > 0 {
                                self.line(&format!("     (i32.const {}) (i32.add)", f.offset));
                            }
                            self.line("     (i32.const 0) (i32.store)");
                        }
                    }
                }
            }
        }
    }

    fn emit_discard(&mut self, ty: TypeId) {
        match self.interner.kind(ty) {
            TyKind::Js => self.line("     (call $js_unregister)"),
            _ if self.interner.needs_drop(ty) => self.line("     (call $dream_drop)"),
            _ => self.line("     (drop)"),
        }
    }

    fn emit_print_temp_string(&mut self) {
        self.line("     (local.set $__drop)");
        self.line("     (local.get $__drop)");
        self.line("     (call $print_string)");
        self.line("     (local.get $__drop)");
        self.line("     (call $dream_drop)");
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
            self.line(&format!("     (i32.const {})", size));
            self.line("     (i32.add)");
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
        let vars: Vec<(u32, u32, crate::emit::debug_map::SpillKind)> = dbg
            .vars
            .iter()
            .map(|v| (v.local, v.global, v.spill))
            .collect();
        for (local, global, kind) in vars {
            self.emit_var_spill(local, global, kind);
        }
        self.line(&format!(
            "     (call $__dbg_line (i32.const {}) (i32.const {}))",
            file_id, line
        ));
    }

    /// Spills one named local into its `i64` pool global, widening/reinterpreting to preserve the
    /// exact bits so the host can decode the value back using the variable's declared kind.
    fn emit_var_spill(
        &mut self,
        local: u32,
        global: u32,
        kind: crate::emit::debug_map::SpillKind,
    ) {
        use crate::emit::debug_map::SpillKind as K;
        let value = match kind {
            K::I64 => format!("(local.get ${})", local),
            K::F64 => format!("(i64.reinterpret_f64 (local.get ${}))", local),
            K::F32 => format!(
                "(i64.extend_i32_u (i32.reinterpret_f32 (local.get ${})))",
                local
            ),
            // i32 locals (ints, bools, chars, enums, string/aggregate/reference pointers): keep the
            // exact 32 bits via an unsigned extend.
            K::I32 => format!("(i64.extend_i32_u (local.get ${}))", local),
        };
        self.line(&format!("     (global.set $__dbg_v{} {})", global, value));
    }

    fn emit_assign(&mut self, place: &Place, rvalue: &Rvalue, drop_old: bool) {
        match place {
            Place::Local(l) => {
                let ty = self.func.local_ty(*l);
                if matches!(self.interner.kind(ty), TyKind::Js) {
                    self.emit_js_local_assign(l.0, rvalue);
                    return;
                }
                if self.interner.is_value_type(ty) {
                    let l0 = l.0;
                    match self.frame.kind(*l) {
                        Some(ValueLocalKind::Owning) => {
                            self.emit_value_store(
                                |s| s.line(&format!("     (local.get ${})", l0)),
                                ty,
                                rvalue,
                            );
                        }
                        // A borrow/param value local just holds an address: rebind it to the source
                        // value's address (no copy, no drop).
                        _ => {
                            match rvalue {
                                Rvalue::Use(o) => self.emit_operand_addr(o),
                                _ => self.emit_rvalue(rvalue),
                            }
                            self.line(&format!("     (local.set ${})", l0));
                        }
                    }
                    return;
                }
                self.emit_rvalue(rvalue);
                self.line(&format!("     (local.set ${})", l.0));
            }
            Place::Global(g) => {
                if let Some(&ty) = self.global_tys.get(&g.0) {
                    if matches!(self.interner.kind(ty), TyKind::Js) {
                        self.emit_js_global_assign(g.0, rvalue);
                        return;
                    }
                    if self.interner.is_value_type(ty) {
                        let g0 = g.0;
                        self.emit_value_store(
                            |s| s.line(&format!("     (global.get $g{})", g0)),
                            ty,
                            rvalue,
                        );
                        return;
                    }
                }
                self.emit_rvalue(rvalue);
                self.line(&format!("     (global.set $g{})", g.0));
            }
            Place::Field { base, field } => {
                if let Some(f) = self.field_layout_full(*base, *field) {
                    let (off, fty) = (f.offset, f.ty);
                    let b = *base;
                    if !drop_old || realloc_self_store(place, rvalue) {
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
                    self.emit_place_store_no_release_old(
                        ety,
                        move |s| s.elem_addr(b, ety, &idx, uc),
                        rvalue,
                    );
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
                self.emit_place_store_no_release_old(
                    ety,
                    move |s| s.line(&format!("     (local.get ${})", p.0)),
                    rvalue,
                );
            }
        }
    }

    fn emit_f32x4_addr(&mut self, arr: &Operand, index: &Operand) {
        self.emit_operand(arr);
        self.line("     (i32.const 4)");
        self.line("     (i32.add)");
        self.emit_operand(index);
        self.line("     (i32.const 2)");
        self.line("     (i32.shl)");
        self.line("     (i32.add)");
    }

    /// Stores `rvalue` into a memory place of type `ty` whose address is produced by `addr`.
    /// Value structs copy in place; heap refs store the pointer; `js` handles
    /// retain-on-copy / unregister-old.
    fn emit_place_store(&mut self, ty: TypeId, addr: impl Fn(&mut Self), rvalue: &Rvalue) {
        self.emit_place_store_ref(ty, addr, rvalue, true);
    }

    /// Same as [`Self::emit_place_store`] for the realloc-self-store idiom (old block already
    /// consumed by `$realloc`).
    fn emit_place_store_no_release_old(
        &mut self,
        ty: TypeId,
        addr: impl Fn(&mut Self),
        rvalue: &Rvalue,
    ) {
        self.emit_place_store_ref(ty, addr, rvalue, false);
    }

    fn emit_place_store_ref(
        &mut self,
        ty: TypeId,
        addr: impl Fn(&mut Self),
        rvalue: &Rvalue,
        drop_old: bool,
    ) {
        if self.interner.is_value_type(ty) {
            self.emit_value_store(addr, ty, rvalue);
            return;
        }
        if matches!(self.interner.kind(ty), TyKind::Js) {
            self.emit_js_place_store(addr, rvalue);
            return;
        }
        if self.interner.is_reference(ty) {
            // Emit the rvalue first so nested `$malloc` cannot leave `$__slot` pointing at a
            // stale address if a helper reuses `$__rel`.
            self.emit_rvalue(rvalue);
            self.line("     (local.set $__src)");
            addr(self);
            self.line("     (local.set $__slot)");
            if drop_old {
                self.line("     (local.get $__slot)");
                self.line(&format!("     ({})", self.load_instr(ty)));
                self.line("     (call $dream_drop)");
            }
            self.line("     (local.get $__slot)");
            self.line("     (local.get $__src)");
            self.line(&format!("     ({})", self.store_instr(ty)));
            return;
        }
        addr(self);
        self.emit_rvalue(rvalue);
        self.line(&format!("     ({})", self.store_instr(ty)));
    }

    /// Assigns a `js` local: `$js_retain` when copying an existing handle, then store, then
    /// `$js_unregister` the previous occupant.
    fn emit_js_local_assign(&mut self, local: u32, rvalue: &Rvalue) {
        self.line(&format!("     (local.get ${}) (local.set $__rel)", local));
        self.emit_rvalue(rvalue);
        self.line("     (local.set $__src)");
        if matches!(rvalue, Rvalue::Use(Operand::Copy(_))) {
            self.line(
                "     (local.get $__src) (if (then (local.get $__src) (call $js_retain)))",
            );
        }
        self.line(&format!("     (local.get $__src) (local.set ${})", local));
        self.line(
            "     (local.get $__rel) (if (then (local.get $__rel) (call $js_unregister)))",
        );
    }

    fn emit_js_global_assign(&mut self, gid: u32, rvalue: &Rvalue) {
        self.line(&format!(
            "     (global.get $g{}) (local.set $__rel)",
            gid
        ));
        self.emit_rvalue(rvalue);
        self.line("     (local.set $__src)");
        if matches!(rvalue, Rvalue::Use(Operand::Copy(_))) {
            self.line(
                "     (local.get $__src) (if (then (local.get $__src) (call $js_retain)))",
            );
        }
        self.line(&format!(
            "     (local.get $__src) (global.set $g{})",
            gid
        ));
        self.line(
            "     (local.get $__rel) (if (then (local.get $__rel) (call $js_unregister)))",
        );
    }

    /// Memory store of a `js` handle: retain on Copy, store, unregister previous.
    fn emit_js_place_store(&mut self, addr: impl Fn(&mut Self), rvalue: &Rvalue) {
        addr(self);
        self.line("     (i32.load) (local.set $__rel)");
        self.emit_rvalue(rvalue);
        self.line("     (local.set $__src)");
        if matches!(rvalue, Rvalue::Use(Operand::Copy(_))) {
            self.line(
                "     (local.get $__src) (if (then (local.get $__src) (call $js_retain)))",
            );
        }
        addr(self);
        self.line("     (local.get $__src) (i32.store)");
        self.line(
            "     (local.get $__rel) (if (then (local.get $__rel) (call $js_unregister)))",
        );
    }

    /// Stores `value` into the object under construction (`$__obj + offset`).
    pub(super) fn store_at_obj(&mut self, offset: u32, value_ty: TypeId, value: &Operand) {
        if self.interner.is_value_type(value_ty) {
            let value = value.clone();
            self.emit_value_copy(
                |s| {
                    s.line("     (local.get $__obj)");
                    if offset > 0 {
                        s.line(&format!("     (i32.const {}) (i32.add)", offset));
                    }
                },
                |s| s.emit_operand_addr(&value),
                value_ty,
            );
            return;
        }
        if matches!(self.interner.kind(value_ty), TyKind::Js) {
            self.emit_operand(value);
            self.line("     (local.set $__src)");
            if matches!(value, Operand::Copy(_)) {
                self.line(
                    "     (local.get $__src) (if (then (local.get $__src) (call $js_retain)))",
                );
            }
            self.line("     (local.get $__obj)");
            if offset > 0 {
                self.line(&format!("     (i32.const {})", offset));
                self.line("     (i32.add)");
            }
            self.line("     (local.get $__src)");
            self.line(&format!("     ({})", self.store_instr(value_ty)));
            return;
        }
        if self.interner.is_reference(value_ty) {
            // Mirror `emit_place_store` for reference slots: materialize the source, then
            // recompute the destination so nested construction that reuses scratch locals
            // cannot store through a stale `$__rel`.
            self.emit_operand(value);
            self.line("     (local.set $__src)");
            self.line("     (local.get $__obj)");
            if offset > 0 {
                self.line(&format!("     (i32.const {})", offset));
                self.line("     (i32.add)");
            }
            self.line("     (local.set $__rel)");
            self.line("     (local.get $__rel)");
            self.line("     (local.get $__src)");
            self.line(&format!("     ({})", self.store_instr(value_ty)));
            return;
        }
        self.line("     (local.get $__obj)");
        if offset > 0 {
            self.line(&format!("     (i32.const {})", offset));
            self.line("     (i32.add)");
        }
        self.emit_operand(value);
        self.line(&format!("     ({})", self.store_instr(value_ty)));
    }

    /// Writes a zero of `field_ty`'s width into the object under construction (`$__obj + offset`).
    /// Used to clear a struct before a user constructor runs (reused heap blocks are not zeroed).
    pub(super) fn zero_at_obj(&mut self, offset: u32, field_ty: TypeId) {
        self.line("     (local.get $__obj)");
        if offset > 0 {
            self.line(&format!("     (i32.const {})", offset));
            self.line("     (i32.add)");
        }
        let zero = match self.store_instr(field_ty) {
            "f64.store" => "(f64.const 0)",
            "f32.store" => "(f32.const 0)",
            "i64.store" => "(i64.const 0)",
            _ => "(i32.const 0)",
        };
        self.line(&format!("     {}", zero));
        self.line(&format!("     ({})", self.store_instr(field_ty)));
    }
}
