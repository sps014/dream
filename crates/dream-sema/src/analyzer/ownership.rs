//! Move tracking for parameters and task captures.
//!
//! Storing an unmarked RC param into a field/index moves it (and makes the param a sink once
//! modes are inferred): further uses must go through the destination (e.g. `this.items.length`,
//! not `items.length` after `this.items = items`). Callers still copy into sink *calls* when the
//! argument is live afterward (MIR [`RcInsertion`]), so call sites do not invalidate the binding.

use crate::symbol_table::SymbolTable;
use dream_abi::names::THIS_NAME;
use dream_diagnostics::{Diagnostic, DiagnosticBag};
use dream_syntax::nodes::{ExpressionNode, Type};
use dream_text::text_span::TextSpan;
use indexmap::IndexMap;

/// Where a local's value went, so a later use can say what to write instead.
#[derive(Debug, Clone)]
pub(super) struct MoveSite {
    pub(super) span: Option<TextSpan>,
    pub(super) kind: MoveKind,
}

#[derive(Debug, Clone)]
pub(super) enum MoveKind {
    /// Stored into a field or element; `place` is its source spelling when it has a simple one.
    Stored { place: Option<String> },
    /// Captured by a `Task` body, which now owns it on another thread.
    Task,
}

pub(super) type MovedLocals = IndexMap<String, MoveSite>;

/// Joins the moved sets of two control-flow paths: moved on either path is moved after the join.
pub(super) fn merge_moved(mut into: MovedLocals, other: &MovedLocals) -> MovedLocals {
    for (name, site) in other {
        into.entry(name.clone()).or_insert_with(|| site.clone());
    }
    into
}

/// Source spelling of a simple place (`this.items`, `node.next`, `slots[i]`), for diagnostics.
pub(super) fn place_text(expr: &ExpressionNode<'_>) -> Option<String> {
    match expr {
        ExpressionNode::Identifier(id) => Some(id.text.clone()),
        ExpressionNode::MemberAccess(obj, member) => {
            Some(format!("{}.{}", place_text(obj)?, member.text))
        }
        ExpressionNode::IndexAccess(array, index) => {
            Some(format!("{}[{}]", place_text(array)?, place_text(index)?))
        }
        ExpressionNode::Parenthesized(_, inner) => place_text(inner),
        ExpressionNode::Literal(Type::Integer(token)) => Some(token.text.clone()),
        _ => None,
    }
}

impl<'a> super::Analyzer<'a> {
    pub(super) fn clear_moved_locals(&mut self) {
        self.moved_locals.clear();
    }

    pub(super) fn unmark_moved_local(&mut self, name: &str) {
        self.moved_locals.shift_remove(name);
    }

    pub(super) fn check_local_not_moved(
        &self,
        name: &str,
        span: Option<TextSpan>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let Some(site) = self.moved_locals.get(name) else {
            return;
        };
        let line = site
            .span
            .map(|s| format!(" on line {}", s.line_no))
            .unwrap_or_default();
        let diagnostic = Diagnostic::new(
            format!("use of '{name}' after it was moved"),
            span,
            diagnostics.file_path.clone(),
        )
        .with_code("use-after-move");
        let diagnostic = match &site.kind {
            MoveKind::Stored { place: Some(place) } => diagnostic
                .with_note(format!("'{name}' was moved into '{place}'{line}; '{place}' owns it now"))
                .with_help(format!("use '{place}' here instead of '{name}'"))
                .with_help(format!(
                    "or read what you need from '{name}' before the line that stores it"
                )),
            MoveKind::Stored { place: None } => diagnostic
                .with_note(format!("'{name}' was moved into a field or element{line}"))
                .with_help("read it through the place that now holds it")
                .with_help(format!(
                    "or read what you need from '{name}' before the line that stores it"
                )),
            MoveKind::Task => diagnostic
                .with_note(format!("'{name}' was moved into a task body{line}; the task owns it now"))
                .with_help(format!(
                    "finish using '{name}' before starting the task, or have the task return what you need"
                ))
                .with_help("make the type a 'shared class' if both threads must use the same object"),
        };
        diagnostics.report(diagnostic);
    }

    /// After `place = <ident>` where `ident` is a sink RC parameter: mark it moved.
    pub(super) fn note_sink_store_move(
        &mut self,
        rhs: &ExpressionNode<'a>,
        rhs_ty: &Type,
        symbols: &SymbolTable,
        place: Option<String>,
    ) {
        let ExpressionNode::Identifier(id) = rhs else {
            return;
        };
        if id.text == THIS_NAME || id.text == "_" {
            return;
        }
        if rhs_ty.is_unknown() || !self.type_is_rc_tracked(rhs_ty) {
            return;
        }
        if !symbols.is_sink_parameter(&id.text) {
            return;
        }
        self.moved_locals.insert(
            id.text.clone(),
            MoveSite {
                span: Some(id.position),
                kind: MoveKind::Stored { place },
            },
        );
    }

    pub(super) fn note_task_capture_move(&mut self, name: String, span: Option<TextSpan>) {
        self.moved_locals.insert(
            name,
            MoveSite {
                span,
                kind: MoveKind::Task,
            },
        );
    }

    /// Hook after a resolved sink call. Unmarked sinks copy when live (see MIR), so this does
    /// not mark caller locals moved.
    pub(super) fn note_sink_arg_moves(
        &mut self,
        _args: &[ExpressionNode<'a>],
        _arg_type_names: &[dream_types::TypeId],
        _is_take: &[bool],
        _skip_receiver: bool,
        _diagnostics: &mut DiagnosticBag,
    ) {
    }

    pub(super) fn snapshot_moved(&self) -> MovedLocals {
        self.moved_locals.clone()
    }

    pub(super) fn restore_moved(&mut self, saved: MovedLocals) {
        self.moved_locals = saved;
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
