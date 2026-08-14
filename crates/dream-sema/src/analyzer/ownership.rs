//! Unique/move tracking: heap locals become unusable after they are assigned away or `move`d.

use super::Analyzer;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{ExpressionNode, Type};
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_syntax::token::token_kind::TokenKind;

impl<'a> Analyzer<'a> {
    pub(super) fn type_needs_unique_drop(&mut self, ty: &Type) -> bool {
        if ty.is_unknown() {
            return false;
        }
        let id = self.type_ctx.lower(ty);
        self.type_ctx.interner.needs_drop(id)
    }

    pub(super) fn check_moved_ident(
        &self,
        id: &SyntaxToken,
        diagnostics: &mut DiagnosticBag,
    ) {
        if self.moved_locals.contains(id.text.as_str()) {
            diagnostics.report_error(
                format!("use of moved value '{}'", id.text),
                Some(id.position),
            );
        }
    }

    pub(super) fn unmove_local(&mut self, name: &str) {
        self.moved_locals.remove(name);
    }

    /// Marks the identifier at the root of `expr` as moved when `ty` is an owning heap type.
    pub(super) fn note_move_expr(&mut self, expr: &ExpressionNode<'a>, ty: &Type) {
        if !self.type_needs_unique_drop(ty) {
            return;
        }
        if let Some(name) = move_source_name(expr) {
            self.moved_locals.insert(name);
        }
    }
}

pub(super) fn move_source_name(expr: &ExpressionNode<'_>) -> Option<String> {
    let mut e = expr;
    loop {
        match e {
            ExpressionNode::Parenthesized(_, inner) => e = inner,
            ExpressionNode::Unary(op, inner) if op.kind == TokenKind::MoveToken => e = inner,
            ExpressionNode::Identifier(tok) => return Some(tok.text.clone()),
            _ => return None,
        }
    }
}
