//! Call-argument and call-shape emission: direct-call argument widening, dynamic interface dispatch
//! (value and sret ABIs), and indirect sret dispatch. Methods on the parent module's private
//! `Emitter`; the ones reached from sibling emitter modules (statements/terminator/value_struct) are
//! widened to the emitter-module scope.

use super::*;

/// Maximum number of `$__raiN` scratch locals reserved in the function prologue for the rooted
/// call-arg spill path (see [`Emitter::emit_rooted_call_args`]). Sized well beyond realistic call
/// arities in Dream code; call sites exceeding it are a compiler bug, not a language limit.
pub(in crate::emit::emitter) const MAX_ARG_SPILL_SLOTS: usize = 16;

impl Emitter<'_> {
    /// Emits a call's arguments, applying implicit numeric widening to each so a narrower argument
    /// (e.g. an `int`/`float` passed to a `double` parameter) matches the callee's WASM signature.
    /// A `fun(...)`-typed parameter is unboxed to its raw funcidx (`i32.load` of the funcbox) only
    /// for **host imports** — they expect a bare table index. Internal MIR callees keep the funcbox
    /// pointer; their bodies already call `funcbox_funcidx` / `funcbox_env` on it. Stripping the box
    /// for an in-module call would pass a table index as a heap address and dispatch the wrong
    /// function (see `List.sort` + `sort_by(desc)` in the same function). Falls back to a plain push
    /// when the callee's parameter types are unknown (intrinsics without a sig entry).
    ///
    /// **GC safety.** If any argument is reference-typed, evaluating it and leaving it on the WASM
    /// operand stack while a *later* argument evaluates (or while the callee's prologue runs) risks
    /// letting a Gen0 evacuation move the underlying object without updating the on-stack copy.
    /// The spill path below pushes every ref arg through the GC root table before assembling the
    /// call frame, then reloads the (possibly forwarded) pointers in argument order and drops the
    /// ephemeral root frame immediately before the call itself. Purely scalar/value args stay on
    /// the fast path — no rooting overhead — because their evaluation cannot trigger a collection.
    pub(in crate::emit::emitter) fn emit_call_args(
        &mut self,
        callee: &crate::Callee,
        args: &[Operand],
    ) {
        let params = self.sigs.get(&(callee.def, callee.args.clone())).cloned();
        let is_host_import = !self.func_table.contains_key(&(callee.def, callee.args.clone()));
        let ref_arg_count = args
            .iter()
            .filter(|a| self.interner.is_reference(self.operand_ty(a)))
            .count();
        if ref_arg_count == 0 {
            for (i, a) in args.iter().enumerate() {
                self.emit_operand(a);
                self.apply_arg_widen(a, i, params.as_deref(), is_host_import);
            }
            return;
        }
        self.emit_rooted_call_args(args, params.as_deref(), is_host_import);
    }

    /// Root every reference-typed argument in a contiguous root-table frame before assembling the
    /// call, then re-emit each argument in call order (reference args reload their forwarded value
    /// from the root table; scalars re-emit their side-effect-free operand). Pops the frame just
    /// before the callsite so the callee's own prologue does not observe leftover entries.
    fn emit_rooted_call_args(
        &mut self,
        args: &[Operand],
        params: Option<&[TypeId]>,
        is_host_import: bool,
    ) {
        // Ephemeral `$__raiN` slots — one per ref arg. Widened past a handful of args a call site
        // gets pathological; the analyzer never accepts that shape (params are per-declaration),
        // so an ICE here is a compiler bug, not a language limit.
        if args.len() > MAX_ARG_SPILL_SLOTS {
            crate::internal_error!(
                "call has {} args (>{}); Emitter arg-spill scratches not sized for this",
                args.len(),
                MAX_ARG_SPILL_SLOTS,
            );
        }
        self.line(&format!(
            "     (i32.const {}) (i32.load) (local.set $__rcsave)",
            crate::abi::GC_ROOT_COUNT_ADDR
        ));
        let mut ref_slot: Vec<Option<usize>> = Vec::with_capacity(args.len());
        let mut next_slot = 0usize;
        for a in args {
            let aty = self.operand_ty(a);
            if self.interner.is_reference(aty) {
                self.emit_operand(a);
                self.line(&format!(
                    "     (call $gc_root_push) (local.set $__rai{})",
                    next_slot
                ));
                ref_slot.push(Some(next_slot));
                next_slot += 1;
            } else {
                ref_slot.push(None);
            }
        }
        for (i, a) in args.iter().enumerate() {
            match ref_slot[i] {
                Some(slot) => self.line(&format!(
                    "     (local.get $__rai{}) (call $gc_root_get)",
                    slot
                )),
                None => self.emit_operand(a),
            }
            self.apply_arg_widen(a, i, params, is_host_import);
        }
        // Drop the ephemeral arg-root frame before the call. The values are already on the WASM
        // stack (forwarded at `gc_root_get` time), so a collection triggered inside the callee
        // will root them again through the callee's own prologue.
        self.line("     (local.get $__rcsave) (call $gc_root_pop)");
    }

    /// Applies the `emit_call_args` widening/funcbox-unbox rule to the `i`-th argument already
    /// left on the WASM operand stack — factored so both the fast path and the rooted spill path
    /// stay in lockstep with the parameter-type rules.
    fn apply_arg_widen(
        &mut self,
        a: &Operand,
        i: usize,
        params: Option<&[TypeId]>,
        is_host_import: bool,
    ) {
        let Some(pty) = params.and_then(|p| p.get(i)) else {
            return;
        };
        if matches!(self.interner.kind(*pty), TyKind::Func(..)) {
            if is_host_import {
                self.line("     (i32.load)");
            }
        } else {
            self.emit_numeric_conv(self.operand_ty(a), *pty);
        }
    }

    /// Emits an indirect call through a raw function-table index `target` (already unboxed by the
    /// caller — see `Analyzer::hir_set_indirect_call_expr`): pushes `args` then `target`, then
    /// dispatches through `$__ft` with the signature derived from `sig` (the interned `fun(...)`
    /// shape). Returns the callee's WASM result type name (`None` for `void`/an unrecognized
    /// signature), so callers know whether the trampoline left a value on the stack to consume or
    /// discard. Shared by [`Rvalue::IndirectCall`] (leaves the result on the stack) and
    /// [`Statement::IndirectCall`] (drops it — see `emit_stmt`).
    ///
    /// See [`Self::emit_call_args`] for the reference-argument spilling scheme; the same GC-safety
    /// concern applies here: reference args sitting only on the WASM operand stack while later
    /// work runs risk staleness across an evacuating collect.
    pub(in crate::emit::emitter) fn emit_indirect_call(
        &mut self,
        target: &Operand,
        sig: dream_types::TypeId,
        args: &[Operand],
    ) -> Option<&'static str> {
        let ref_arg_count = args
            .iter()
            .filter(|a| self.interner.is_reference(self.operand_ty(a)))
            .count();
        if ref_arg_count == 0 {
            for a in args {
                self.emit_operand(a);
            }
        } else {
            self.emit_rooted_call_args(args, None, false);
        }
        // The target itself is a raw table index (i32), materialized right before dispatch — no
        // GC roots to worry about.
        self.emit_operand(target);
        let (sig_name, ret) = func_sig(self.interner, sig)
            .map(|(name, _, ret)| (name, ret))
            .unwrap_or_else(|| ("$sig___v".to_string(), None));
        self.line(&format!("     (call_indirect $__ft (type {}))", sig_name));
        ret
    }

    /// Emits a dynamic interface method call. The receiver is pushed as argument 0, then the real
    /// arguments (widened to the interface method's declared parameter types), then control transfers
    /// to the per-`(interface, method)` dispatch trampoline which looks the concrete implementation up
    /// in the tag-indexed itable and forwards through `$__ft`. The trampoline leaves the result (if
    /// any) on the stack.
    pub(in crate::emit::emitter) fn emit_interface_call(
        &mut self,
        receiver: &Operand,
        iface_id: usize,
        method_slot: usize,
        sig: TypeId,
        args: &[Operand],
    ) {
        self.emit_interface_receiver_args(receiver, sig, args);
        self.line(&format!(
            "     (call ${})",
            iface_dispatch_symbol(iface_id, method_slot)
        ));
    }

    /// Emits a dynamic interface call to a *value*(`struct`/union)-returning method using the sret
    /// ABI: the destination address (produced by `dst`) is pushed as the hidden leading argument,
    /// then the receiver and real arguments, then control transfers to the dispatch trampoline
    /// (which forwards the sret pointer through to the concrete implementation). Mirrors
    /// [`emit_value_sret_call`](Self::emit_value_sret_call) for direct calls.
    pub(in crate::emit::emitter) fn emit_interface_sret_call(
        &mut self,
        dst: impl Fn(&mut Self),
        receiver: &Operand,
        iface_id: usize,
        method_slot: usize,
        sig: TypeId,
        args: &[Operand],
    ) {
        dst(self);
        self.emit_interface_receiver_args(receiver, sig, args);
        self.line(&format!(
            "     (call ${})",
            iface_dispatch_symbol(iface_id, method_slot)
        ));
    }

    /// Pushes an interface call's receiver (argument 0) followed by the real arguments, each widened
    /// to the interface method's declared parameter type. Shared by the value and sret interface-call
    /// paths, which differ only in whether an sret destination precedes the receiver.
    ///
    /// The receiver is always a reference; combined with any reference args this needs the same
    /// GC-safe spilling as [`Self::emit_call_args`]. Take the fast path only when *all* of the
    /// receiver + args are safe to sit on the operand stack — currently that means only when there
    /// are no other reference-typed args at all (the receiver alone can't be evacuated before the
    /// dispatch, since no further work happens after emitting it).
    fn emit_interface_receiver_args(&mut self, receiver: &Operand, sig: TypeId, args: &[Operand]) {
        let param_tys: Vec<TypeId> = match self.interner.kind(sig) {
            TyKind::Func(params, _) => params.clone(),
            _ => Vec::new(),
        };
        let ref_arg_count = args
            .iter()
            .filter(|a| self.interner.is_reference(self.operand_ty(a)))
            .count();
        if ref_arg_count == 0 {
            self.emit_operand(receiver);
            for (i, a) in args.iter().enumerate() {
                self.emit_operand(a);
                // param_tys[0] is the receiver (`this`); real args start at index 1.
                if let Some(pty) = param_tys.get(i + 1) {
                    self.emit_numeric_conv(self.operand_ty(a), *pty);
                }
            }
            return;
        }
        // Rooted path: fold the receiver into the arg vector so the shared spill path handles it
        // uniformly. The params slice shifts by one to line up with the augmented arg list.
        let mut all_args: Vec<Operand> = Vec::with_capacity(args.len() + 1);
        all_args.push(receiver.clone());
        all_args.extend_from_slice(args);
        self.emit_rooted_call_args(&all_args, Some(&param_tys), false);
    }

    /// Emits an indirect (funcref) call to a value-struct-returning target using the sret ABI: the
    /// destination address (`dst`) is passed as the hidden leading argument, then the real
    /// arguments, then the table index dispatched through `$__ft` with `sig`'s sret signature.
    /// Mirrors [`emit_value_sret_call`](Self::emit_value_sret_call) for first-class function values
    /// (e.g. a worker body funcref of type `fun(TIn): TOut` where `TOut` is a struct).
    pub(in crate::emit::emitter) fn emit_indirect_sret_call(
        &mut self,
        dst: impl Fn(&mut Self),
        target: &Operand,
        sig: dream_types::TypeId,
        args: &[Operand],
    ) {
        dst(self);
        for a in args {
            self.emit_operand(a);
        }
        self.emit_operand(target);
        let sig_name = func_sig(self.interner, sig)
            .map(|(name, _, _)| name)
            .unwrap_or_else(|| "$sig___v".to_string());
        self.line(&format!("     (call_indirect $__ft (type {}))", sig_name));
    }
}
