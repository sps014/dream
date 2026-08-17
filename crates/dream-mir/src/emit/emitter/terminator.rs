//! `Terminator` emission (branches, returns, tail calls) for the WAT backend, plus the dynamic `js`
//! call marshaling helper. Split out of `emitter.rs`; these are methods on the parent's private
//! `Emitter`.

use super::*;

impl Emitter<'_> {
    pub(super) fn emit_terminator(&mut self, t: &Terminator) {
        match t {
            Terminator::Goto(b) => self.goto(*b),
            Terminator::If {
                cond,
                then_blk,
                else_blk,
            } => {
                self.emit_operand(cond);
                self.f.if_();
                self.goto(*then_blk);
                self.f.else_();
                self.goto(*else_blk);
                self.f.end();
            }
            Terminator::Switch {
                value,
                targets,
                default,
            } => {
                // Lower to a chain of compares; a real br_table needs contiguous keys.
                for (k, b) in targets {
                    self.emit_operand(value);
                    self.f.i32_const(*k as i32);
                    self.f.i32_eq();
                    self.f.if_();
                    self.goto(*b);
                    self.f.end();
                }
                self.goto(*default);
            }
            Terminator::Return(Some(o)) => {
                self.emit_debug_exit();
                if self.returns_value_struct() {
                    if let Operand::Copy(Place::Local(l)) = o {
                        if self.is_v128_local(*l) {
                            self.f.local_get("__sret");
                            self.f.local_get(&(l.0).to_string());
                            self.f.store(StoreKind::V128, 0);
                            self.emit_frame_teardown();
                            self.f.return_();
                            return;
                        }
                    }
                    let o = o.clone();
                    let ty = self.func.ret;
                    let retain = !matches!(o, Operand::Copy(Place::Local(_)));
                    self.emit_value_copy(
                        |s| s.f.local_get("__sret"),
                        |s| s.emit_operand_addr(&o),
                        ty,
                        retain,
                    );
                    self.emit_frame_teardown();
                    self.f.return_();
                } else {
                    self.emit_operand(o);
                    self.emit_frame_teardown();
                    self.f.return_();
                }
            }
            Terminator::Return(None) => {
                self.emit_debug_exit();
                self.emit_frame_teardown();
                self.f.return_();
            }
            Terminator::TailCall { callee, args } => {
                self.emit_debug_exit();
                let sym = self.callee_symbol(callee);
                if let Some(kind) = async_intrinsic_kind(&sym) {
                    self.emit_async_intrinsic(kind, args);
                    if !self.wasm_returns_value() {
                        self.f.drop_();
                    }
                    self.emit_frame_teardown();
                    self.f.return_();
                } else {
                    let simd_tail = self.returns_value_struct()
                        && self.try_emit_simd_call(
                            callee,
                            args,
                            Some(&|s: &mut Self| s.f.local_get("__sret")),
                            None,
                        )
                        || self.try_emit_simd_call(callee, args, None::<fn(&mut Self)>, None);
                    if simd_tail {
                        self.emit_frame_teardown();
                        self.f.return_();
                    } else {
                        self.emit_call_args(callee, args);
                        self.emit_frame_teardown();
                        self.f.return_call(&sym);
                    }
                }
            }
            Terminator::Unreachable => self.f.unreachable(),
            Terminator::AsyncComplete(_) => self.f.unreachable(),
            Terminator::Await { .. } => self.f.unreachable(),
        }
    }

    /// A CFG edge: set the dispatch PC to the target and loop back to re-dispatch.
    fn goto(&mut self, target: crate::BlockId) {
        self.f.i32_const((target.0) as i32);
        self.f.local_set("__pc");
        self.f.br("__loop");
    }

    /// Emits a dynamic `js` call marshaling its arguments through the shadow stack in one host
    /// crossing (no per-argument boxing, no heap array): save `$__sp`, carve `argc * 16` bytes,
    /// write one 16-byte tagged slot per argument (`[tag][aux][payload]`), call the bridge with
    /// `(target, [viaPtr,] [namePtr,] argsPtr, argc)`, and restore `$__sp` (the result stays on
    /// the WASM stack). The buffer lives below the value-struct frame and is released immediately, so
    /// it is allocation-free and re-entrant (a nested `js` call saves/restores its own `$__sp`).
    pub(super) fn emit_js_call(
        &mut self,
        callee: &crate::Callee,
        target: &Operand,
        via: Option<&Operand>,
        method: Option<&Operand>,
        args: &[(Operand, TypeId)],
    ) {
        use dream_abi::js_abi;
        let argc = args.len() as u32;
        // Save `$__sp` and carve the slot buffer (skipped for a zero-argument call).
        self.f.global_get("__sp");
        self.f.local_set("__jsp");
        if argc > 0 {
            self.f.global_get("__sp");
            self.f.i32_const((argc * js_abi::SLOT_SIZE) as i32);
            self.f.i32_sub();
            self.f.global_set("__sp");
        }
        for (i, (op, ty)) in args.iter().enumerate() {
            let base = (i as u32) * js_abi::SLOT_SIZE;
            let (tag, aux, store) = js_abi::slot_desc(self.interner, *ty);
            self.emit_slot_word(base, tag);
            self.emit_slot_word(base + js_abi::SLOT_AUX_OFFSET, aux);
            // Payload: the argument value, stored at its natural width. A `fun(...)` value is
            // always the 2-word `[funcidx][env]` box described in `runtime/closure.wat` (see
            // `funcbox_new`/`funcbox_funcidx`) - never a bare table index - so a `FUNC` slot must
            // dereference it to the funcidx word the host's `decodeJsSlots`/`callback()` expects;
            // storing the box pointer itself would hand the host a heap address, not a table index.
            self.f.global_get("__sp");
            self.f
                .i32_const((base + js_abi::SLOT_PAYLOAD_OFFSET) as i32);
            self.f.i32_add();
            self.emit_operand(op);
            if tag == js_abi::tag::FUNC {
                self.f.load(LoadKind::I32, 0);
            }
            self.f.store(
                match store {
                    "i64.store" => StoreKind::I64,
                    "f64.store" => StoreKind::F64,
                    "f32.store" => StoreKind::F32,
                    _ => StoreKind::I32,
                },
                0,
            );
        }
        // Bridge args: target, [viaPtr,] [namePtr,] argsPtr (= current $__sp), argc.
        self.emit_operand(target);
        if let Some(prop) = via {
            self.emit_operand(prop);
        }
        if let Some(name) = method {
            self.emit_operand(name);
        }
        self.f.global_get("__sp");
        self.f.i32_const((argc) as i32);
        let sym = self.callee_symbol(callee);
        self.f.call(&sym);
        // Release the buffer; the call's result remains beneath on the WASM stack.
        self.f.local_get("__jsp");
        self.f.global_set("__sp");
    }

    /// Stores an `i32` `value` (a WAT snippet leaving one `i32` on the stack) into the argument-slot
    /// buffer at byte offset `off` from `$__sp` — used by [`emit_js_call`](Self::emit_js_call) for a
    /// slot's `tag`/`aux` header words.
    fn emit_slot_word(&mut self, off: u32, value: i32) {
        self.f.global_get("__sp");
        self.f.i32_const(off as i32);
        self.f.i32_add();
        self.f.i32_const(value);
        self.f.store(StoreKind::I32, 0);
    }
}
