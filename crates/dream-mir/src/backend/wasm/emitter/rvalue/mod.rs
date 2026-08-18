//! Rvalue (expression) emission for the WAT backend: the big `emit_rvalue` dispatch. The helpers it
//! drives are split by concern into sibling submodules:
//! - [`casts`]: `Cast` lowering (value struct boxing, primitive box/unbox, struct<->js marshaling)
//!   and the WASM numeric-conversion helpers shared with call-argument widening.
//! - [`calls`]: call-argument widening and the interface/indirect call-shape emission (value + sret).
//!
//! All are methods on the parent module's private `Emitter`, so they can reach its other helpers.

use super::*;

mod calls;
mod casts;

impl Emitter<'_> {
    pub(super) fn emit_rvalue(&mut self, rvalue: &Rvalue) {
        match rvalue {
            Rvalue::Use(o) => self.emit_operand(o),
            Rvalue::Select {
                cond,
                then_val,
                else_val,
            } => {
                // WASM `select` pops [val1, val2, cond] and yields val1 when cond != 0.
                self.emit_operand(then_val);
                self.emit_operand(else_val);
                self.emit_operand(cond);
                self.f.select();
            }
            Rvalue::Binary(op, a, b) => {
                let ta = self.operand_ty(a);
                let tb = self.operand_ty(b);
                // String equality compares contents, not pointers, via the runtime `$string_eq`.
                let str_eq = matches!(op, BinOp::Eq | BinOp::Ne)
                    && matches!(self.interner.kind(ta), TyKind::Prim(PrimTy::String));
                if str_eq {
                    self.emit_operand(a);
                    self.emit_operand(b);
                    self.f.call("string_eq");
                    if matches!(op, BinOp::Ne) {
                        self.f.i32_eqz();
                    }
                } else {
                    // The operation runs at one WASM width, so widen the narrower operand to the
                    // common numeric type (e.g. `someLong > 0` widens the `int` literal `0` to i64).
                    // Without this a mixed-width pair emits e.g. `i64.gt_s` over an i32 operand,
                    // which fails WASM validation.
                    let common = self.wider_numeric(ta, tb);
                    // Integer `/`/`%` by zero would otherwise hit WASM's own opaque
                    // `integer divide by zero` trap (no message, no location); check explicitly and
                    // route through `$dream_panic` instead so the failure is diagnosable like every
                    // other runtime check.
                    if matches!(op, BinOp::Div | BinOp::Rem)
                        && matches!(
                            wasm_val_ty(self.interner, common),
                            ValType::I32 | ValType::I64
                        )
                    {
                        self.emit_operand(b);
                        self.emit_numeric_conv(tb, common);
                        match wasm_val_ty(self.interner, common) {
                            ValType::I64 => self.f.i64_eqz(),
                            _ => self.f.i32_eqz(),
                        }
                        self.f.if_();
                        self.emit_panic(super::super::panic_msgs::DIVIDE_BY_ZERO);
                        self.f.end();
                    }
                    self.emit_operand(a);
                    self.emit_numeric_conv(ta, common);
                    self.emit_operand(b);
                    self.emit_numeric_conv(tb, common);
                    self.f.nullary(self.binop_instr(*op, common));
                    // `byte` shares WASM's `i32` register type (see `wasm_types.rs`), so unlike
                    // every other integer primitive it is *not* naturally kept in range by its own
                    // arithmetic instruction — `i32.add`/`i32.sub`/`i32.mul`/`i32.shl` can produce a
                    // result outside `[0, 255]` that would otherwise only get truncated back into
                    // range on its next store to `byte`-typed memory (`i32.store8`), leaving it
                    // silently out-of-range in between (e.g. read back by a subsequent comparison).
                    // Masking immediately after the op — the same wrapping semantics every other
                    // integer type already gets for free from its native WASM width — keeps a
                    // `byte` value in range at every step, matching the overflow policy documented
                    // in `docs/language/primitives.md`.
                    if matches!(self.interner.kind(common), TyKind::Prim(PrimTy::Byte))
                        && matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Shl)
                    {
                        self.f.i32_const(255);
                        self.f.i32_and();
                    }
                }
            }
            Rvalue::Unary(op, a) => {
                let ty = self.operand_ty(a);
                match op {
                    UnOp::Neg => {
                        // No `neg` for integers in WASM: 0 - x.
                        if matches!(
                            self.interner.kind(ty),
                            TyKind::Prim(PrimTy::Float | PrimTy::Double)
                        ) {
                            self.emit_operand(a);
                            match wasm_val_ty(self.interner, ty) {
                                ValType::F64 => self.f.f64_neg(),
                                _ => self.f.f32_neg(),
                            }
                        } else {
                            match wasm_val_ty(self.interner, ty) {
                                ValType::I64 => self.f.i64_const(0),
                                ValType::F32 => self.f.f32_const(0.0),
                                ValType::F64 => self.f.f64_const(0.0),
                                _ => self.f.i32_const(0),
                            }
                            self.emit_operand(a);
                            match wasm_val_ty(self.interner, ty) {
                                ValType::I64 => self.f.i64_sub(),
                                ValType::F32 => self.f.f32_sub(),
                                ValType::F64 => self.f.f64_sub(),
                                _ => self.f.i32_sub(),
                            }
                        }
                    }
                    UnOp::Not => {
                        self.emit_operand(a);
                        self.f.i32_eqz();
                    }
                    UnOp::BitNot => {
                        // No dedicated bitwise-complement instruction in WASM: `x ^ -1` flips every
                        // bit of the value's native width, which is exactly `~x` for a two's-complement
                        // integer.
                        self.emit_operand(a);
                        match wasm_val_ty(self.interner, ty) {
                            ValType::I64 => {
                                self.f.i64_const(-1);
                                self.f.i64_xor();
                            }
                            _ => {
                                self.f.i32_const(-1);
                                self.f.i32_xor();
                            }
                        }
                        // `byte` shares WASM's `i32` register (see the `Rvalue::Binary` byte-masking
                        // comment above): flipping all 32 bits leaves the top 24 non-zero, so mask
                        // back down to `[0, 255]` immediately, same as every other byte-producing op.
                        if matches!(self.interner.kind(ty), TyKind::Prim(PrimTy::Byte)) {
                            self.f.i32_const(255);
                            self.f.i32_and();
                        }
                    }
                }
            }
            Rvalue::Call { callee, args } => {
                let sym = self.callee_symbol(callee);
                if let Some(kind) = async_intrinsic_kind(&sym) {
                    self.emit_async_intrinsic(kind, args);
                } else if self.try_emit_simd_call(callee, args, None::<fn(&mut Self)>, None) {
                    // Vector<T> lane_count / sum: result left on the stack.
                } else {
                    self.emit_call_args(callee, args);
                    self.f.call(&sym);
                    // `$funcbox_env` is an i32 load; stdlib types it as `long` so native pointers fit.
                    if sym.trim_start_matches('$') == "funcbox_env" {
                        self.emit_numeric_conv(self.interner.byte(), callee.ret);
                    }
                }
            }
            Rvalue::IndirectCall { target, sig, args } => {
                self.emit_indirect_call(target, *sig, args);
            }
            Rvalue::InterfaceCall {
                receiver,
                iface_id,
                method_slot,
                sig,
                args,
                ..
            } => {
                self.emit_interface_call(receiver, *iface_id, *method_slot, *sig, args);
            }
            Rvalue::JsCall {
                callee,
                target,
                via,
                method,
                args,
            } => {
                self.emit_js_call(callee, target, via.as_ref(), method.as_ref(), args);
            }
            Rvalue::FuncRef(callee) => {
                // A function value is its slot index in the module function table. The table is
                // built from every referenced function, so a miss means it diverged from MIR
                // (compiler bug); trap loudly rather than silently referencing slot 0.
                let idx = self
                    .func_table
                    .get(&(callee.def, callee.args.clone()))
                    .copied()
                    .unwrap_or_else(|| {
                        crate::internal_error!(
                            "funcref to def{} missing from the function table",
                            callee.def.0
                        )
                    });
                self.f.i32_const(idx as i32);
            }
            Rvalue::New {
                def,
                ty,
                ctor,
                args,
            } => {
                // `$malloc(data_size, tag)` returns a data pointer with refcount 1.
                let info = self.layouts.get(*ty).map(|l| {
                    (
                        l.size,
                        l.fields
                            .iter()
                            .map(|f| (f.offset, f.ty))
                            .collect::<Vec<_>>(),
                    )
                });
                if let Some((size, fields)) = info {
                    // `@shared class` instances carry one extra header-adjacent word right past the
                    // last field (the reentrant lock word backing `lock (obj) { ... }` and atomic
                    // retain/release — see `src/mir/abi.rs`'s shared-lock-word note). Reused heap
                    // blocks are not zeroed, so it must be zeroed explicitly, same as any field.
                    let is_shared = self.interner.is_shared_type(*ty);
                    let alloc_size = if is_shared {
                        size + crate::abi::HEADER_LOCK_WORD_SIZE
                    } else {
                        size
                    };
                    self.f.i32_const((alloc_size) as i32);
                    self.f.i32_const(self.type_tag(*ty, *def));
                    self.f.call("malloc");
                    self.f.local_set("__obj");
                    if is_shared {
                        self.zero_at_obj(size, self.interner.int());
                    }
                    if let Some(ctor) = ctor {
                        // A user `constructor(this, args...)` sets the fields itself. Reused heap
                        // blocks are not zeroed, so zero every field first (a constructor that leaves a
                        // field unset must observe 0/null), then call it; the object is the result.
                        for &(off, fty) in &fields {
                            self.zero_at_obj(off, fty);
                        }
                        self.f.local_get("__obj");
                        for arg in args {
                            self.emit_operand(arg);
                        }
                        let sym = self.callee_symbol(&crate::Callee {
                            def: *ctor,
                            args: vec![],
                            ret: self.interner.void(),
                            take_params: vec![],
                        });
                        self.f.call(&sym);
                        self.f.local_get("__obj");
                    } else {
                        // Implicit zero-arg default constructor: leave every field at its zero
                        // value. Reused heap blocks are not zeroed, so zero each field explicitly.
                        let _ = args;
                        for &(off, fty) in &fields {
                            self.zero_at_obj(off, fty);
                        }
                        self.f.local_get("__obj");
                    }
                } else {
                    crate::internal_error!("missing layout for struct allocation (type {:?})", ty);
                }
            }
            Rvalue::UnionNew {
                def,
                ty,
                variant,
                args,
            } => {
                // A union value is one heap block `[discriminant: i32][payload...]`, sized to the
                // largest variant so any variant fits. `variant` is the discriminant; allocate,
                // write it at offset 0, then store the payload at the variant's field offsets.
                let layout = self.layouts.union(*ty).and_then(|u| {
                    let size = u.size;
                    u.variants
                        .iter()
                        .find(|v| v.discriminant as usize == *variant)
                        .map(|v| {
                            (
                                size,
                                v.fields
                                    .iter()
                                    .map(|f| (f.offset, f.ty))
                                    .collect::<Vec<_>>(),
                            )
                        })
                });
                if let Some((size, fields)) = layout {
                    // The analyzer already checked the variant's arity, so the argument list and the
                    // variant's field slots must line up; a mismatch would silently drop or
                    // misplace payload words.
                    debug_assert_eq!(
                        args.len(),
                        fields.len(),
                        "union def{} variant {} arity ({} args) disagrees with its layout ({} fields)",
                        def.0, variant, args.len(), fields.len()
                    );
                    self.f.i32_const((size) as i32);
                    self.f.i32_const(self.type_tag(*ty, *def));
                    self.f.call("malloc");
                    self.f.local_set("__obj");
                    self.f.local_get("__obj");
                    self.f.i32_const(*variant as i32);
                    self.f.store(StoreKind::I32, 0);
                    for (i, arg) in args.iter().enumerate() {
                        let &(off, fty) = fields.get(i).unwrap_or_else(|| {
                            crate::internal_error!(
                                "union def{} variant {} has no field slot for argument {}",
                                def.0,
                                variant,
                                i
                            )
                        });
                        self.store_at_obj(off, fty, arg);
                    }
                    self.f.local_get("__obj");
                } else {
                    // A union that survived analysis always has a registered layout; a miss is a
                    // compiler bug, so trap loudly rather than emitting a null pointer.
                    crate::internal_error!(
                        "missing layout for union def{} variant {}",
                        def.0,
                        variant
                    );
                }
            }
            Rvalue::ArrayLit { elem_ty, elems } => {
                // Array block: `[len: i32][elem0][elem1]...`; the length is the first word (matching
                // `ArrayLen`), elements follow at stride `elem_size`.
                let (esize, _) = scalar_size(self.interner, *elem_ty);
                // `[len:i32] + count * esize`. A literal big enough to overflow u32 is not
                // representable in source, but guard the arithmetic so a bug can never emit a
                // silently-truncated (undersized) allocation.
                let size = (elems.len() as u32)
                    .checked_mul(esize)
                    .and_then(|payload| payload.checked_add(crate::abi::LEN_PREFIX_SIZE))
                    .unwrap_or_else(|| {
                        crate::internal_error!(
                            "array literal size overflows u32 ({} elems x {} bytes)",
                            elems.len(),
                            esize
                        )
                    });
                self.f.i32_const((size) as i32);
                self.f.i32_const(ARRAY_TAG);
                self.f.call("malloc");
                self.f.local_set("__obj");
                self.f.local_get("__obj");
                self.f.i32_const((elems.len()) as i32);
                self.f.store(StoreKind::I32, 0);
                for (i, e) in elems.iter().enumerate() {
                    self.store_at_obj(
                        crate::abi::LEN_PREFIX_SIZE + esize * (i as u32),
                        *elem_ty,
                        e,
                    );
                }
                self.f.local_get("__obj");
            }
            Rvalue::ArrayNew { elem_ty, len } => {
                // Block: `[len: i32][elem0..]`, zero-initialized (recycled freelist blocks are not
                // zeroed, and reference-typed releases rely on null slots).
                let (esize, _) = scalar_size(self.interner, *elem_ty);
                self.emit_operand(len);
                self.f.local_set("__len");
                // size = LEN_PREFIX + len * esize
                self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                self.f.local_get("__len");
                self.f.i32_const((esize) as i32);
                self.f.i32_mul();
                self.f.i32_add();
                self.f.i32_const(ARRAY_TAG);
                self.f.call("malloc");
                self.f.local_set("__obj");
                self.f.local_get("__obj");
                self.f.local_get("__len");
                self.f.store(StoreKind::I32, 0);
                // memory.fill(dst = obj+4, 0, len*esize)
                self.f.local_get("__obj");
                self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                self.f.i32_add();
                self.f.i32_const(0);
                self.f.local_get("__len");
                self.f.i32_const((esize) as i32);
                self.f.i32_mul();
                self.f.memory_fill();
                self.f.local_get("__obj");
            }
            Rvalue::Tuple { .. } => {
                // Value tuples are always stored via `emit_value_store` / `construct_value_tuple`.
                crate::internal_error!("tuple rvalue emitted as a stack value")
            }
            Rvalue::ArrayLen(o) => {
                self.emit_operand(o);
                self.f.load(LoadKind::I32, 0);
            }
            Rvalue::ToBytes { value, ty } => {
                // `T[]` (a blittable-element array) has no static byte size — its length is a
                // runtime value — so it gets its own dynamic-length raw copy instead of falling
                // into the fixed-size scalar/value-struct paths below: allocate a `byte[]` sized to
                // `len * elem_size` and `memory.copy` the source array's payload (everything after
                // its own length word) straight across. This is a deep copy of the element bytes,
                // never the array pointer, so the two sides never alias the same heap block (the
                // `@shared class` wire-sharing approach was reverted for exactly that aliasing risk;
                // arrays instead get fresh, independent storage on each end, like value structs do).
                if let TyKind::Array(elem_ty) = self.interner.kind(*ty) {
                    let (esize, _) = scalar_size(self.interner, *elem_ty);
                    self.emit_operand(value);
                    self.f.local_set("__src");
                    self.f.local_get("__src");
                    self.f.load(LoadKind::I32, 0);
                    self.f.i32_const((esize) as i32);
                    self.f.i32_mul();
                    self.f.local_set("__len");
                    self.f.local_get("__len");
                    self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                    self.f.i32_add();
                    self.f.i32_const(ARRAY_TAG);
                    self.f.call("malloc");
                    self.f.local_set("__obj");
                    self.f.local_get("__obj");
                    self.f.local_get("__len");
                    self.f.store(StoreKind::I32, 0);
                    self.f.local_get("__obj");
                    self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                    self.f.i32_add();
                    self.f.local_get("__src");
                    self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                    self.f.i32_add();
                    self.f.local_get("__len");
                    self.f.memory_copy();
                    self.f.local_get("__obj");
                    return;
                }
                // Allocate a `byte[]` of `[len: i32][size bytes]`. `byte` elements are one byte, so
                // the length word is the byte count. A value-struct `T` is already addressed (never
                // loaded), so its bytes are `memory.copy`'d from that address; a scalar `T` (int,
                // double, bool, ...) is a raw WASM value on the stack with no address of its own, so
                // it is written directly with the matching store instruction instead.
                let size = self.value_size(*ty);
                self.f
                    .i32_const((crate::abi::LEN_PREFIX_SIZE + size) as i32);
                self.f.i32_const(ARRAY_TAG);
                self.f.call("malloc");
                self.f.local_set("__obj");
                self.f.local_get("__obj");
                self.f.i32_const((size) as i32);
                self.f.store(StoreKind::I32, 0);
                if self.interner.is_value_type(*ty) {
                    // memory.copy(dst = obj+4, src = value address, size)
                    self.f.local_get("__obj");
                    self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                    self.f.i32_add();
                    self.emit_operand_addr(value);
                    self.f.i32_const((size) as i32);
                    self.f.memory_copy();
                } else {
                    self.f.local_get("__obj");
                    self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                    self.f.i32_add();
                    self.emit_operand(value);
                    self.f.store(self.store_kind(*ty), 0);
                }
                self.f.local_get("__obj");
            }
            Rvalue::FromBytes { bytes, ty } => {
                // The `T[]` counterpart of the dynamic-length `ToBytes` array path above: the wire
                // buffer's byte length divides evenly by the element size (it was produced by that
                // same `ToBytes` path), so the element count is recovered from it, a fresh array
                // block is allocated, and the payload is `memory.copy`'d straight across.
                if let TyKind::Array(elem_ty) = self.interner.kind(*ty) {
                    let (esize, _) = scalar_size(self.interner, *elem_ty);
                    self.emit_operand(bytes);
                    self.f.local_set("__src");
                    self.f.local_get("__src");
                    self.f.load(LoadKind::I32, 0);
                    self.f.local_set("__len");
                    self.f.local_get("__len");
                    self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                    self.f.i32_add();
                    self.f.i32_const(ARRAY_TAG);
                    self.f.call("malloc");
                    self.f.local_set("__obj");
                    self.f.local_get("__obj");
                    self.f.local_get("__len");
                    self.f.i32_const((esize) as i32);
                    self.f.i32_div_u();
                    self.f.store(StoreKind::I32, 0);
                    self.f.local_get("__obj");
                    self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                    self.f.i32_add();
                    self.f.local_get("__src");
                    self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                    self.f.i32_add();
                    self.f.local_get("__len");
                    self.f.memory_copy();
                    self.f.local_get("__obj");
                    return;
                }
                // A scalar `T` (int, double, bool, ...) is reconstructed by loading it straight out
                // of the buffer's payload (which starts after the 4-byte length prefix) - no
                // allocation needed, since the result is a raw WASM value, not a heap reference. A
                // value-struct `T` needs its own private storage (a value struct is always
                // addressed), so it is copied into a fresh `T`-tagged block instead.
                let size = self.value_size(*ty);
                if self.interner.is_value_type(*ty) {
                    let tag = self.type_tag(*ty, dream_types::DefId(0));
                    self.f.i32_const((size) as i32);
                    self.f.i32_const(tag);
                    self.f.call("malloc");
                    self.f.local_set("__obj");
                    // memory.copy(dst = obj, src = bytes+4, size)
                    self.f.local_get("__obj");
                    self.emit_operand(bytes);
                    self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                    self.f.i32_add();
                    self.f.i32_const((size) as i32);
                    self.f.memory_copy();
                    self.f.local_get("__obj");
                } else {
                    self.emit_operand(bytes);
                    self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                    self.f.i32_add();
                    self.f.load(self.load_kind(*ty), 0);
                }
            }
            Rvalue::ArrayRealloc {
                elem_ty,
                array,
                new_len,
            } => {
                // `$realloc(ptr, new_total, tag)` preserves the overlapping prefix but never zeroes
                // a grown tail (unlike `$malloc`'s caller-side `memory.fill` in `ArrayNew` above), so
                // that fill is done here explicitly, guarded so a shrink/no-grow never runs it with
                // a wrapped-negative (huge unsigned) size.
                let (esize, _) = scalar_size(self.interner, *elem_ty);
                self.emit_operand(array);
                self.f.local_set("__obj");
                self.f.local_get("__obj");
                self.f.load(LoadKind::I32, 0);
                self.f.local_set("__old_len");
                self.emit_operand(new_len);
                self.f.local_set("__len");
                self.f.local_get("__obj");
                self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                self.f.local_get("__len");
                self.f.i32_const((esize) as i32);
                self.f.i32_mul();
                self.f.i32_add();
                self.f.i32_const(ARRAY_TAG);
                self.f.call("realloc");
                self.f.local_set("__obj");
                self.f.local_get("__obj");
                self.f.local_get("__len");
                self.f.store(StoreKind::I32, 0);
                self.f.local_get("__len");
                self.f.local_get("__old_len");
                self.f.i32_gt_s();
                self.f.if_();
                self.f.local_get("__obj");
                self.f.i32_const(crate::abi::LEN_PREFIX_SIZE as i32);
                self.f.i32_add();
                self.f.local_get("__old_len");
                self.f.i32_const((esize) as i32);
                self.f.i32_mul();
                self.f.i32_add();
                self.f.i32_const(0);
                self.f.local_get("__len");
                self.f.local_get("__old_len");
                self.f.i32_sub();
                self.f.i32_const((esize) as i32);
                self.f.i32_mul();
                self.f.memory_fill();
                self.f.end();
                self.f.local_get("__obj");
            }
            Rvalue::CharAt(s, i, unchecked) => self.emit_char_at(s, i, *unchecked),
            Rvalue::ByteAt(s, i, unchecked) => self.emit_byte_at(s, i, *unchecked),
            Rvalue::Concat(parts) => {
                let n = parts.len();
                for p in parts {
                    self.emit_operand(p);
                }
                match n {
                    2 => self.f.call("concat_strings"),
                    3 => self.f.call("concat_strings3"),
                    n => panic!("ICE: Concat expects 2 or 3 parts, got {}", n),
                }
            }
            Rvalue::ConcatInt {
                prefix,
                value,
                suffix,
            } => {
                self.emit_operand(prefix);
                self.emit_operand(value);
                self.emit_operand(suffix);
                self.f.call("concat_str_int_str");
            }
            Rvalue::ToString(o) => {
                self.emit_operand(o);
                let oty = self.operand_ty(o);
                // A value struct/union is addressed inline (no heap tag header), so its `to_string`
                // is dispatched statically to the concrete `$<Type>_to_string` rather than routed
                // through the tag-dispatching `$object_to_string`.
                if self.interner.is_value_type(oty) {
                    if let Some(name) = self.value_name(oty) {
                        self.f.call(&format!("{}_to_string", name));
                        return;
                    }
                }
                // A `string` is already its own `to_string`; every other type has a formatter.
                if let Some(call) = value_to_string_call(self.interner, oty) {
                    self.f.call(&call);
                }
            }
            Rvalue::EnumName { value, arms } => {
                let empty = self.string_addr("");
                self.emit_operand(value);
                self.f.local_set("__len");
                // Nested `value == disc ? strptr : (...)`, terminating in the empty string.
                for (disc, name) in arms {
                    let ptr = self.string_addr(name);
                    self.f.local_get("__len");
                    self.f.i32_const(*disc as i32);
                    self.f.i32_eq();
                    self.f.if_ty(BlockTy::I32);
                    self.f.i32_const((ptr) as i32);
                    self.f.else_();
                }
                self.f.i32_const((empty) as i32);
                for _ in arms {
                    self.f.end();
                }
            }
            Rvalue::HashCode(o) => {
                self.emit_operand(o);
                let oty = self.operand_ty(o);
                if self.interner.is_value_type(oty) {
                    if let Some(name) = self.value_name(oty) {
                        self.f.call(&format!("{}_hash_code", name));
                        return;
                    }
                }
                match self.interner.kind(oty) {
                    // Integer-family values (and enums) are their own hash.
                    TyKind::Prim(
                        PrimTy::Int | PrimTy::UInt | PrimTy::Bool | PrimTy::Char | PrimTy::Byte,
                    )
                    | TyKind::Enum(_) => {}
                    TyKind::Prim(PrimTy::Long | PrimTy::ULong) => self.f.call("hash_long"),
                    TyKind::Prim(PrimTy::Float) => self.f.i32_reinterpret_f32(),
                    TyKind::Prim(PrimTy::Double) => self.f.call("hash_double"),
                    TyKind::Prim(PrimTy::String) => self.f.call("hash_string"),
                    _ => self.f.call("object_hash_code"),
                }
            }
            Rvalue::StrLen(o) => {
                self.emit_operand(o);
                self.f.load(LoadKind::I32, 0);
            }
            Rvalue::StrByteSize(o) => {
                self.emit_operand(o);
                self.f.load(LoadKind::I32, 0);
                self.f.i32_const(1);
                self.f.i32_shl();
            }
            Rvalue::Cast(o, from, to) => self.emit_cast(o, *from, *to),
            Rvalue::IsType(o, target) => {
                self.emit_operand(o);
                self.f.call("object_tag");
                // The analyzer only admits `is` against a type with a concrete runtime tag; a
                // `None` here means an unsupported target slipped through (compiler bug). Comparing
                // against 0 would silently answer the wrong question, so fail loudly instead.
                let tag = runtime_tag_for(self.interner, self.tags, *target).unwrap_or_else(|| {
                    crate::internal_error!("`is` target type {:?} has no runtime tag", target)
                });
                self.f.i32_const(tag);
                self.f.i32_eq();
            }
            Rvalue::Discriminant(o) => {
                // The discriminant is the `i32` at offset 0 of the union block.
                self.emit_operand(o);
                self.f.load(LoadKind::I32, 0);
            }
            Rvalue::UnionField {
                base,
                ty,
                variant,
                field,
            } => {
                let slot = self.layouts.union(*ty).and_then(|u| {
                    u.variants
                        .iter()
                        .find(|v| v.discriminant as usize == *variant)
                        .and_then(|v| v.fields.get(*field))
                        .map(|f| (f.offset, f.ty))
                });
                if let Some((off, fty)) = slot {
                    self.emit_operand(base);
                    if off > 0 {
                        self.f.i32_const((off) as i32);
                        self.f.i32_add();
                    }
                    // A value-struct payload is addressed inline (its bytes live in the union block),
                    // so reading it yields the payload address rather than a load.
                    if !self.interner.is_value_type(fty) {
                        self.f.load(self.load_kind(fty), 0);
                    }
                } else {
                    crate::internal_error!(
                        "missing layout for union payload (type {:?}, variant {}, field {})",
                        ty,
                        variant,
                        field
                    );
                }
            }
        }
    }
}
