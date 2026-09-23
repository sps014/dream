//! `checked { body }` / `unchecked { body }` — lexical integer overflow mode.

use super::*;
use dream_hir::Overflow;
use dream_syntax::nodes::{OverflowMode, StatementNode};
use dream_syntax::token::syntax_token::SyntaxToken;

impl<'a> Analyzer<'a> {
    /// The body is its own scope, but its HIR is spliced into the enclosing block: the mode only
    /// changes how arithmetic nodes are stamped, not control flow.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::analyzer) fn analyze_overflow_block(
        &mut self,
        mode: OverflowMode,
        keyword: &SyntaxToken,
        body: &[StatementNode<'a>],
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        has_parent_while: bool,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let overflow = if self.current_function_is_gpu {
            if mode == OverflowMode::Checked {
                diagnostics.report_error(
                    "'checked' cannot be used in GPU code: shader arithmetic always wraps"
                        .to_string(),
                    Some(keyword.position),
                );
            }
            Overflow::Wrapping
        } else {
            match mode {
                OverflowMode::Checked => Overflow::Checked,
                OverflowMode::Unchecked => Overflow::Wrapping,
            }
        };
        let saved = std::mem::replace(&mut self.overflow, overflow);
        let body_scope = self.branch_scope(symbol_table);
        self.hir_open_block();
        let result = self.analyze_body(
            body,
            parent_function,
            Some(&body_scope),
            has_parent_while,
            diagnostics,
        );
        let stmts = self.hir_close_block();
        self.overflow = saved;
        result?;
        for stmt in stmts {
            self.hir_push_stmt(stmt);
        }
        Ok(())
    }
}
