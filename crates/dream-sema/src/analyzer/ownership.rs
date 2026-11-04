//! Move tracking for task captures: a local a `Task` body captures belongs to that task.
//! Moves of unique values everywhere else are checked on the HIR by `move_check`.

use dream_diagnostics::{Diagnostic, DiagnosticBag};
use dream_syntax::nodes::Type;
use dream_text::text_span::TextSpan;
use indexmap::IndexMap;

/// Where a `Task` body captured a local.
#[derive(Debug, Clone)]
pub(super) struct MoveSite {
    pub(super) span: Option<TextSpan>,
}

pub(super) type MovedLocals = IndexMap<String, MoveSite>;

/// Joins the moved sets of two control-flow paths: moved on either path is moved after the join.
pub(super) fn merge_moved(mut into: MovedLocals, other: &MovedLocals) -> MovedLocals {
    for (name, site) in other {
        into.entry(name.clone()).or_insert_with(|| site.clone());
    }
    into
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
        let diagnostic = diagnostic
            .with_note(format!("'{name}' was moved into a task body{line}; the task owns it now"))
            .with_help(format!(
                "finish using '{name}' before starting the task, or have the task return what you need"
            ))
            .with_help("make the type a 'shared class' if both threads must use the same object");
        diagnostics.report(diagnostic);
    }

    pub(super) fn note_task_capture_move(&mut self, name: String, span: Option<TextSpan>) {
        self.moved_locals.insert(name, MoveSite { span });
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
}
