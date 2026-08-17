//! Call-argument and call-shape emission: direct-call argument widening, dynamic interface dispatch
//! (value and sret ABIs), and indirect sret dispatch. Methods on the parent module's private
//! `Emitter`; the ones reached from sibling emitter modules (statements/terminator/value_struct) are
//! widened to the emitter-module scope.

use super::*;

impl Emitter<'_> {
    /// Emits a call's arguments, applying implicit numeric widening to each so a narrower argument
    /// (e.g. an `int`/`float` passed to a `double` parameter) matches the callee's WASM signature.
    /// A `fun(...)`-typed parameter is unboxed to its raw funcidx (`i32.load` of the funcbox) only
    /// for **host imports** — they expect a bare table index. Internal MIR callees keep the funcbox
    /// pointer; their bodies already call `funcbox_funcidx` / `funcbox_env` on it. Stripping the box
    /// for an in-module call would pass a table index as a heap address and dispatch the wrong
    /// function (see `List.sort` + `sort_by(desc)` in the same function). Falls back to a plain push
    /// when the callee's parameter types are unknown (intrinsics without a sig entry).
    pub(in crate::emit::emitter) fn emit_call_args(
        &mut self,
        callee: &crate::Callee,
        args: &[Operand],
    ) {
        let params = self.sigs.get(&(callee.def, callee.args.clone())).cloned();
        let is_host_import = !self
            .defined_funcs
            .contains(&(callee.def, callee.args.clone()));
        for (i, a) in args.iter().enumerate() {
            self.emit_place_value_arg_retain(a);
            self.emit_call_arg(a);
            if let Some(pty) = params.as_ref().and_then(|p| p.get(i)) {
                if matches!(self.interner.kind(*pty), TyKind::Func(..)) {
                    if is_host_import {
                        // Boxed `fun(...)` value → raw funcref-table index the host's `callback()` expects.
                        self.f.load(LoadKind::I32, 0);
                    }
                } else {
                    self.emit_numeric_conv(self.operand_ty(a), *pty);
                }
            }
        }
    }

    /// Pushes a call argument: a `Vector<T>` `v128` local is spilled so the callee sees an `i32`
    /// pointer, matching the value-struct ABI used by wrapper methods that were not SIMD-lowered.
    pub(in crate::emit::emitter) fn emit_call_arg(&mut self, a: &Operand) {
        if let Operand::Copy(Place::Local(l)) = a {
            if self.is_v128_local(*l) {
                self.emit_v128_as_ptr(*l);
                return;
            }
        }
        self.emit_operand(a);
    }

    /// Emits an indirect call through a raw function-table index `target` (already unboxed by the
    /// caller — see `Analyzer::hir_set_indirect_call_expr`): pushes `args` then `target`, then
    /// dispatches through `$__ft` with the signature derived from `sig` (the interned `fun(...)`
    /// shape). Returns the callee's WASM result type name (`None` for `void`/an unrecognized
    /// signature), so callers know whether the trampoline left a value on the stack to consume or
    /// discard. Shared by [`Rvalue::IndirectCall`] (leaves the result on the stack) and
    /// [`Statement::IndirectCall`] (drops it — see `emit_stmt`).
    pub(in crate::emit::emitter) fn emit_indirect_call(
        &mut self,
        target: &Operand,
        sig: dream_types::TypeId,
        args: &[Operand],
    ) -> Option<&'static str> {
        for a in args {
            self.emit_place_value_arg_retain(a);
            self.emit_call_arg(a);
        }
        self.emit_operand(target);
        let (sig_name, ret) = func_sig(self.interner, sig)
            .map(|(name, _, ret)| (name, ret))
            .unwrap_or_else(|| ("$sig___v".to_string(), None));
        self.f.call_indirect(&sig_name, "__ft");
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
        self.f.call(&iface_dispatch_symbol(iface_id, method_slot));
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
        self.f.call(&iface_dispatch_symbol(iface_id, method_slot));
    }

    /// Pushes an interface call's receiver (argument 0) followed by the real arguments, each widened
    /// to the interface method's declared parameter type. Shared by the value and sret interface-call
    /// paths, which differ only in whether an sret destination precedes the receiver.
    fn emit_interface_receiver_args(&mut self, receiver: &Operand, sig: TypeId, args: &[Operand]) {
        let param_tys: Vec<TypeId> = match self.interner.kind(sig) {
            TyKind::Func(params, _) => params.clone(),
            _ => Vec::new(),
        };
        self.emit_call_arg(receiver);
        for (i, a) in args.iter().enumerate() {
            self.emit_place_value_arg_retain(a);
            self.emit_call_arg(a);
            // param_tys[0] is the receiver (`this`); real args start at index 1.
            if let Some(pty) = param_tys.get(i + 1) {
                self.emit_numeric_conv(self.operand_ty(a), *pty);
            }
        }
    }

    /// Emits an indirect (funcref) call to a value-struct-returning target using the sret ABI: the
    /// destination address (`dst`) is passed as the hidden leading argument, then the real
    /// arguments, then the table index dispatched through `$__ft` with `sig`'s sret signature.
    /// Mirrors [`emit_value_sret_call`](Self::emit_value_sret_call) for first-class function values
    /// (e.g. a worker body funcref of type `fun(): TOut` where `TOut` is a struct).
    pub(in crate::emit::emitter) fn emit_indirect_sret_call(
        &mut self,
        dst: impl Fn(&mut Self),
        target: &Operand,
        sig: dream_types::TypeId,
        args: &[Operand],
    ) {
        dst(self);
        for a in args {
            self.emit_place_value_arg_retain(a);
            self.emit_call_arg(a);
        }
        self.emit_operand(target);
        let sig_name = func_sig(self.interner, sig)
            .map(|(name, _, _)| name)
            .unwrap_or_else(|| "$sig___v".to_string());
        self.f.call_indirect(&sig_name, "__ft");
    }
}
