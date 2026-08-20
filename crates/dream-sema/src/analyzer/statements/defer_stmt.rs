//! `defer { body }` / `defer(q) { body }` — opt-in last-ref destroy queue.

use super::*;
use dream_syntax::nodes::StatementNode;
use dream_syntax::token::token_kind::TokenKind;

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn analyze_defer(
        &mut self,
        budget: &Option<ExpressionNode<'a>>,
        body: &[StatementNode<'a>],
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        has_parent_while: bool,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let uint_ty = Type::UInt(super::super::synthetic_token(
            TokenKind::DataTypeToken,
            "uint",
        ));
        let budget_hir = if let Some(q) = budget {
            let saved = self.current_expected_type.take();
            self.current_expected_type = Some(uint_ty.clone());
            let q_ty = self
                .analyze_expression(q, parent_function, symbol_table, diagnostics)
                .unwrap_or(Type::Unknown);
            self.current_expected_type = saved;
            if !q_ty.is_unknown() {
                let span = q.position().unwrap_or_else(super::super::empty_span);
                self.compare_data_type(&uint_ty, &q_ty, &span, diagnostics)?;
            }
            self.hir_take()
        } else {
            None
        };
        let body_scope = self.branch_scope(symbol_table);
        self.hir_open_block();
        self.analyze_body(
            body,
            parent_function,
            Some(&body_scope),
            has_parent_while,
            diagnostics,
        )?;
        let body_hir = self.hir_close_block();
        for stmt in body {
            self.forbid_await_in_stmt(stmt, "'await' cannot be used inside 'defer'", diagnostics);
        }
        self.hir_defer(budget_hir, body_hir);
        Ok(())
    }
}
