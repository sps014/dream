//! Ownership helpers for sink-default ABI.
//!
//! Unmarked RC params are sinks at the callee. Callers still copy into sink *calls* when the
//! argument is live afterward (MIR [`RcInsertion`]) — so call sites do not invalidate the
//! binding. Storing a sink param into a field/index **does** move it: further uses must go
//! through the destination (e.g. `this.items.length`, not `items.length` after
//! `this.items = items`).

use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{ExpressionNode, FunctionNode, Type};
use dream_text::text_span::TextSpan;
use std::collections::HashSet;

impl<'a> super::Analyzer<'a> {
    pub(super) fn clear_moved_locals(&mut self) {
        self.moved_locals.clear();
    }

    pub(super) fn unmark_moved_local(&mut self, name: &str) {
        self.moved_locals.remove(name);
    }

    pub(super) fn check_local_not_moved(
        &self,
        name: &str,
        span: Option<TextSpan>,
        diagnostics: &mut DiagnosticBag,
    ) {
        if self.moved_locals.contains(name) {
            diagnostics.report_error(
                format!(
                    "use of '{name}' after move; the value was stored into a field or index — use that place instead"
                ),
                span,
            );
        }
    }

    /// After `place = <ident>` where `ident` is a sink RC parameter: mark it moved.
    pub(super) fn note_sink_store_move(
        &mut self,
        rhs: &ExpressionNode<'a>,
        rhs_ty: &Type,
        parent_function: &FunctionNode<'a>,
    ) {
        let ExpressionNode::Identifier(id) = rhs else {
            return;
        };
        if id.text == "this" || id.text == "_" {
            return;
        }
        if rhs_ty.is_unknown() || !self.type_is_rc_tracked(rhs_ty) {
            return;
        }
        if !Self::is_sink_param(&id.text, parent_function) {
            return;
        }
        self.moved_locals.insert(id.text.clone());
    }

    /// Hook after a resolved sink call. Unmarked sinks copy when live (see MIR), so this does
    /// not mark caller locals moved.
    pub(super) fn note_sink_arg_moves(
        &mut self,
        _args: &[ExpressionNode<'a>],
        _arg_type_names: &[String],
        _is_take: &[bool],
        _skip_receiver: bool,
        _diagnostics: &mut DiagnosticBag,
    ) {
    }

    pub(super) fn snapshot_moved(&self) -> HashSet<String> {
        self.moved_locals.clone()
    }

    pub(super) fn restore_moved(&mut self, saved: HashSet<String>) {
        self.moved_locals = saved;
    }

    fn is_sink_param(name: &str, parent_function: &FunctionNode<'a>) -> bool {
        parent_function
            .parameters
            .iter()
            .any(|p| p.name.text == name && !p.is_ref && !p.is_borrow && p.name.text != "this")
    }

    /// True when `ty` is a managed heap value that can cross a thread boundary by pointer
    /// hand-off (single-owner move): non-value class instances, arrays, unions, interfaces, and
    /// the `object` top type. Value structs are inline copies, not heap refs, so they don't
    /// transfer; blittables and strings are already `shared`.
    pub(super) fn type_is_transferable_managed(&mut self, ty: &Type) -> bool {
        if ty.is_unknown() {
            return false;
        }
        let tid = self.type_ctx.lower(ty);
        match self.type_ctx.interner.kind(tid) {
            dream_types::TyKind::Array(_)
            | dream_types::TyKind::Object
            | dream_types::TyKind::Union(_, _)
            | dream_types::TyKind::Interface(_, _) => true,
            dream_types::TyKind::Struct(def, _) => !self.type_ctx.interner.is_value_def(*def),
            _ => false,
        }
    }

    fn type_is_rc_tracked(&mut self, ty: &Type) -> bool {
        let id = self.type_ctx.lower(ty);
        self.type_ctx.interner.is_rc_tracked(id)
    }
}
