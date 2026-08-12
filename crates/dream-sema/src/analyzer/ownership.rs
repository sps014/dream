//! Ownership helpers (post-ARC).
//!
//! Heap references are plain shared refs. Unmarked parameters are not sinks; `borrow` is
//! accepted as an ignored synonym of unmarked. There is no use-after-move-on-sink check.

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
        _name: &str,
        _span: Option<TextSpan>,
        _diagnostics: &mut DiagnosticBag,
    ) {
        // No use-after-move under GC shared-ref ABI.
    }

    /// No-op: sink-on-store moves were an ARC ownership rule.
    pub(super) fn note_sink_store_move(
        &mut self,
        _rhs: &ExpressionNode<'a>,
        _rhs_ty: &Type,
        _parent_function: &FunctionNode<'a>,
    ) {
    }

    /// No-op: sink argument moves were an ARC ownership rule.
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
}
