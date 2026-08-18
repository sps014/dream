//! Async-coroutine emission for the WAT backend: the poll-completion path, the state-machine
//! dispatch that drives an async body's suspend/resume, and the async CFG terminator / intrinsic
//! helpers. Split out of `emitter.rs`; these are methods on the parent's private `Emitter`.

use super::*;
use crate::async_emit::AsyncSlots;

fn slot_mem_load(wt: &str) -> LoadKind {
    match wt {
        "f64" => LoadKind::F64,
        "f32" => LoadKind::F32,
        "i64" => LoadKind::I64,
        _ => LoadKind::I32,
    }
}

fn slot_mem_store(wt: &str) -> StoreKind {
    match wt {
        "f64" => StoreKind::F64,
        "f32" => StoreKind::F32,
        "i64" => StoreKind::I64,
        _ => StoreKind::I32,
    }
}

impl Emitter<'_> {
    /// Completes the current coroutine: drops frame-resident value(`struct`) locals, then
    /// `$dream_complete($self, value)` and returns `0` (the poll result).
    ///
    /// RC locals are released by MIR `Release` stmts inserted before `AsyncComplete` (see
    /// `RcInsertion`); emitting another bulk release here would double-free aliases and
    /// use-after-free a returned local when packing `F_RESULT`.
    fn emit_poll_complete(&mut self, value: Option<&Operand>) {
        if let Some(parent) = self.async_parent {
            // Only persistent user value locals (params + declared `let`s); trailing synthetic temps
            // are transient. RC ownership is already handled in MIR.
            for (i, decl) in parent
                .locals
                .iter()
                .enumerate()
                .take(self.async_user_locals)
            {
                if self.interner.is_value_type(decl.ty) && self.value_has_glue(decl.ty) {
                    // Value params/locals live at a fixed frame address held in the local; drop glue
                    // releases embedded refs retained when the frame took ownership.
                    self.emit_value_drop(|s| s.f.local_get(&i.to_string()), decl.ty);
                }
            }
        }
        // Debug-info: the coroutine is finishing, so pop its shadow call-stack frame. This is the only
        // exit path (awaits return without popping), so the frame count stays balanced.
        self.emit_debug_exit();
        self.f.local_get("self");
        match value {
            Some(v) => {
                let wt = self.wasm_ty(self.operand_ty(v));
                match wt.as_str() {
                    "i64" => {
                        self.f.local_get("self");
                        self.emit_operand(v);
                        self.f.store(StoreKind::I64, (F_WIDE) as u32);
                        self.f.local_get("self");
                        self.f.i32_const(0);
                    }
                    "f32" => {
                        self.f.local_get("self");
                        self.emit_operand(v);
                        self.f.store(StoreKind::F32, (F_WIDE) as u32);
                        self.f.local_get("self");
                        self.f.i32_const(0);
                    }
                    "f64" => {
                        self.f.local_get("self");
                        self.emit_operand(v);
                        self.f.store(StoreKind::F64, (F_WIDE) as u32);
                        self.f.local_get("self");
                        self.f.i32_const(0);
                    }
                    _ => {
                        self.f.local_get("self");
                        self.emit_operand(v);
                    }
                }
            }
            None => {
                self.f.local_get("self");
                self.f.i32_const(0);
            }
        }
        self.f.call("dream_complete");
        self.f.i32_const(0);
        self.f.return_();
    }

    /// Emits the coroutine poll function: a state-machine dispatch over the whole lowered async body.
    /// On entry the frame-resident locals are restored, then a `$__pc`/`br_table` loop (seeded from
    /// the saved `Future.state`) runs blocks; CFG edges re-dispatch, an [`Terminator::Await`] parks
    /// the task and returns (recording its `resume` block as the next state), and completions run
    /// `$dream_complete`. A block that is some await's `resume` target first binds the settled result.
    pub(super) fn emit_async_state_machine(&mut self, slots: &AsyncSlots, poll_sym: &str) {
        self.f.set_name(poll_sym);
        self.f.param("self", ValType::I32);
        self.f.result(ValType::I32);
        for (i, decl) in self.func.locals.iter().enumerate() {
            self.f
                .local(&i.to_string(), wasm_val_ty(self.interner, decl.ty));
        }
        self.f.local("__obj", ValType::I32);
        self.f.local("__scratch", ValType::I32);
        self.f.local("__len", ValType::I32);
        self.f.local("__rel", ValType::I32);
        self.f.local("__pc", ValType::I32);
        self.f.local("__jsp", ValType::I32);
        self.f.local("__src", ValType::I32);

        // Restore every frame-resident local; reference slots are zeroed after the move so ownership
        // lives in the WASM local (and is not double-freed from the frame) until the next suspend. A
        // value(`struct`) local's bytes live directly at its own fixed frame offset (see
        // `AsyncSlots::value_locals`), so its "value" is just that address, recomputed every poll —
        // never loaded/saved like a plain scalar (its inline storage persists in the frame on its own).
        for (idx, _, wt) in &slots.entries {
            let off = slots.offsets[idx];
            if slots.value_locals.contains_key(idx) {
                self.f.local_get("self");
                self.f.i32_const(off);
                self.f.i32_add();
                self.f.local_set(&idx.to_string());
                continue;
            }
            self.f.local_get("self");
            self.f.load(slot_mem_load(wt), off as u32);
            self.f.local_set(&idx.to_string());
            if slots.ref_locals.contains(idx) {
                self.f.local_get("self");
                self.f.i32_const(0);
                self.f.store(StoreKind::I32, (off) as u32);
            }
        }

        // Blocks that are an await's `resume` target, mapped to the local its result binds to (if any).
        let mut resume_binds: HashMap<u32, Option<crate::Local>> = HashMap::new();
        for block in &self.func.blocks {
            if let Terminator::Await { dest, resume, .. } = &block.terminator {
                resume_binds.insert(resume.0, *dest);
            }
        }

        let n = self.func.blocks.len();
        self.f.local_get("self");
        self.f.load(LoadKind::I32, (F_STATE) as u32);
        self.f.local_set("__pc");
        // Debug-info: announce entry once, on the *initial* poll (state/pc still 0). Resume polls
        // (pc != 0, after an `await`) must not re-push the frame, and suspends must not pop it - the
        // frame is popped only on completion (see `emit_poll_complete`), keeping the shadow call
        // stack balanced across awaits.
        if let Some(dbg) = self.debug_fn {
            self.f.local_get("__pc");
            self.f.i32_eqz();
            self.f.if_();
            self.f.i32_const(dbg.id as i32);
            self.f.call("__dbg_enter");
            self.f.end();
        }
        self.f.block("host_exit");
        self.f.loop_("__loop");
        for i in (0..n).rev() {
            self.f.block(&format!("bb{i}"));
        }
        let labels: Vec<Label> = (0..n).map(|i| Label::Name(format!("bb{i}"))).collect();
        let default = Label::Name(format!("bb{}", n.saturating_sub(1)));
        self.f.local_get("__pc");
        self.f.br_table(labels, default);
        for i in 0..n {
            self.f.end();
            if let Some(dest) = resume_binds.get(&(i as u32)) {
                // Resume point: bind the settled result (`awaiting.result`) before continuing. A
                // wide-scalar (`long`/`float`/`double`) result was boxed by the awaited coroutine's
                // `emit_poll_complete` (see above) since `F_RESULT` only ever holds an `i32`; unbox
                // it back to its native representation here and release the now-consumed box cell.
                // The child Future stays owned by the await operand local (saved/restored across
                // suspend); poll RcInsertion releases it at AsyncComplete — do not free it here.
                let dest_wt = dest.map(|d| self.wasm_ty(self.func.local_ty(d)));
                self.f.local_get("self");
                self.f.load(LoadKind::I32, (F_AWAITING) as u32);
                self.f.load(LoadKind::I32, (F_RESULT) as u32);
                match dest_wt.as_deref() {
                    Some("i64") => {
                        self.f.drop_();
                        self.f.local_get("self");
                        self.f.load(LoadKind::I32, (F_AWAITING) as u32);
                        self.f.load(LoadKind::I64, (F_WIDE) as u32);
                        if let Some(d) = dest {
                            self.f.local_set(&(d.0).to_string());
                        }
                    }
                    Some("f32") => {
                        self.f.drop_();
                        self.f.local_get("self");
                        self.f.load(LoadKind::I32, (F_AWAITING) as u32);
                        self.f.load(LoadKind::F32, (F_WIDE) as u32);
                        if let Some(d) = dest {
                            self.f.local_set(&(d.0).to_string());
                        }
                    }
                    Some("f64") => {
                        self.f.drop_();
                        self.f.local_get("self");
                        self.f.load(LoadKind::I32, (F_AWAITING) as u32);
                        self.f.load(LoadKind::F64, (F_WIDE) as u32);
                        if let Some(d) = dest {
                            self.f.local_set(&(d.0).to_string());
                        }
                        self.f.local_get("__obj");
                        self.f.call("release_generic");
                    }
                    _ => match dest {
                        Some(d) => self.f.local_set(&(d.0).to_string()),
                        None => self.f.drop_(),
                    },
                }
            }
            let block = self.func.block(crate::BlockId(i as u32));
            for stmt in &block.stmts {
                self.emit_stmt(stmt);
            }
            self.emit_async_cfg_terminator(&block.terminator, slots);
        }
        self.f.end(); // loop
        self.f.end(); // $host_exit
                      // Every reachable path suspends (returns) or completes (returns); the tail is unreachable but
                      // keeps the `(result i32)` signature well-typed.
        self.f.unreachable();
    }

    /// Terminator emission inside the coroutine poll dispatch: CFG edges re-dispatch through `$__pc`,
    /// an `Await` parks the task and returns, and completions/returns finish the task.
    fn emit_async_cfg_terminator(&mut self, t: &Terminator, slots: &AsyncSlots) {
        match t {
            Terminator::Goto(_) | Terminator::If { .. } | Terminator::Switch { .. } => {
                self.emit_terminator(t)
            }
            Terminator::Await { future, resume, .. } => {
                // Evaluate the awaited future, park the task on it, save live locals, and return so the
                // scheduler can drive it; the poll re-enters at `resume` when the future settles.
                self.emit_operand(future);
                self.f.local_set("__scratch");
                self.f.local_get("self");
                self.f.local_get("__scratch");
                self.f.store(StoreKind::I32, (F_AWAITING) as u32);
                self.f.local_get("self");
                self.f.i32_const((resume.0) as i32);
                self.f.store(StoreKind::I32, (F_STATE) as u32);
                for (idx, _, wt) in &slots.entries {
                    // A value(`struct`) local's bytes already live at their fixed frame offset (the
                    // local is just `self + offset`, recomputed on restore) — nothing to save.
                    if slots.value_locals.contains_key(idx) {
                        continue;
                    }
                    let off = slots.offsets[idx];
                    self.f.local_get("self");
                    self.f.local_get(&(idx).to_string());
                    self.f.store(slot_mem_store(wt), off as u32);
                }
                self.f.local_get("self");
                self.f.local_get("__scratch");
                self.f.call("dream_await");
                self.f.i32_const(0);
                self.f.return_();
            }
            Terminator::AsyncComplete(v) => {
                let v = v.clone();
                self.emit_poll_complete(v.as_ref());
            }
            // A value `return x;` in an async body lowers to `AsyncComplete`; handle the plain form too.
            Terminator::Return(v) => {
                let v = v.clone();
                self.emit_poll_complete(v.as_ref());
            }
            // TCO never runs on async bodies, so a tail call cannot appear in a poll function.
            Terminator::TailCall { .. } => self.f.unreachable(),
            Terminator::Unreachable => self.f.unreachable(),
        }
    }

    /// Emits `sleep` / `Promise.all|any|race`, leaving a `Future` pointer on the stack.
    pub(super) fn emit_async_intrinsic(&mut self, kind: &str, args: &[Operand]) {
        use dream_abi::intrinsics;
        match kind {
            intrinsics::SLEEP => {
                use crate::async_emit::{F_SLOTS, HOST_POLL_INDEX, KIND_HOST};
                self.emit_operand(&args[0]);
                self.f.local_set("__scratch");
                self.f.i32_const(F_SLOTS);
                self.f.i32_const(HOST_POLL_INDEX);
                self.f.i32_const(KIND_HOST);
                self.f.call("dream_new_future");
                self.f.local_tee("__obj");
                self.f.local_get("__scratch");
                self.f.call("dream_set_timer");
                self.f.local_get("__obj");
            }
            intrinsics::PROMISE_ALL => {
                self.emit_operand(&args[0]);
                self.f.call("dream_all");
            }
            intrinsics::PROMISE_ANY | intrinsics::PROMISE_RACE => {
                self.emit_operand(&args[0]);
                self.f.call("dream_any");
            }
            _ => {}
        }
    }
}
