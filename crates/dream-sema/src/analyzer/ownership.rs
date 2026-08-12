//! Ownership helpers for sink-default ABI.
//!
//! Unmarked RC params are sinks at the callee. Callers move on last use or copy when the
//! argument is still live (MIR [`RcInsertion`]); sema does not treat unmarked sinks as
//! invalidating the caller binding.

use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::ExpressionNode;
use dream_text::text_span::TextSpan;
use std::collections::HashSet;

impl<'a> super::Analyzer<'a> {
    pub(super) fn clear_moved_locals(&mut self) {
        self.moved_locals.clear();
    }

    pub(super) fn check_local_not_moved(
        &self,
        name: &str,
        span: Option<TextSpan>,
        diagnostics: &mut DiagnosticBag,
    ) {
        if self.moved_locals.contains(name) {
            diagnostics.report_error(format!("use of '{name}' after move"), span);
        }
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
}
