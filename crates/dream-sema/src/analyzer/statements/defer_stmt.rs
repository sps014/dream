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
        let fallback_span = budget.as_ref().and_then(|q| q.position());
        for stmt in body {
            self.forbid_await_in_stmt(stmt, "'await' cannot be used inside 'defer'", diagnostics);
            self.forbid_control_flow_in_defer(stmt, diagnostics, fallback_span);
        }
        self.hir_defer(budget_hir, body_hir);
        Ok(())
    }

    fn forbid_control_flow_in_defer(
        &self,
        stmt: &StatementNode<'a>,
        diagnostics: &mut DiagnosticBag,
        fallback: Option<dream_text::text_span::TextSpan>,
    ) {
        let pos = match stmt {
            StatementNode::Return(Some(e)) => e.position().or(fallback),
            StatementNode::Return(None) | StatementNode::Break(_) | StatementNode::Continue(_) => {
                fallback
            }
            _ => fallback,
        };
        match stmt {
            StatementNode::Return(_) => {
                diagnostics.report_error(
                    "'return' cannot be used inside 'defer'".to_string(),
                    pos,
                );
            }
            StatementNode::Break(_) => {
                diagnostics.report_error(
                    "'break' cannot be used inside 'defer'".to_string(),
                    pos,
                );
            }
            StatementNode::Continue(_) => {
                diagnostics.report_error(
                    "'continue' cannot be used inside 'defer'".to_string(),
                    pos,
                );
            }
            StatementNode::IfElse(_, if_body, else_ifs, else_body) => {
                for s in *if_body {
                    self.forbid_control_flow_in_defer(s, diagnostics, fallback);
                }
                for (_, b) in else_ifs {
                    for s in *b {
                        self.forbid_control_flow_in_defer(s, diagnostics, fallback);
                    }
                }
                if let Some(eb) = else_body {
                    for s in *eb {
                        self.forbid_control_flow_in_defer(s, diagnostics, fallback);
                    }
                }
            }
            StatementNode::Switch(_, cases, default_body) => {
                for (_, b) in cases {
                    for s in *b {
                        self.forbid_control_flow_in_defer(s, diagnostics, fallback);
                    }
                }
                if let Some(db) = default_body {
                    for s in *db {
                        self.forbid_control_flow_in_defer(s, diagnostics, fallback);
                    }
                }
            }
            StatementNode::While(_, b) | StatementNode::DoWhile(b, _) => {
                for s in *b {
                    self.forbid_control_flow_in_defer(s, diagnostics, fallback);
                }
            }
            StatementNode::For(_, _, _, b)
            | StatementNode::ForEach(_, _, _, _, b)
            | StatementNode::Lock(_, b)
            | StatementNode::Defer(_, b) => {
                for s in *b {
                    self.forbid_control_flow_in_defer(s, diagnostics, fallback);
                }
            }
            StatementNode::Labeled(_, s) => {
                self.forbid_control_flow_in_defer(s, diagnostics, fallback)
            }
            StatementNode::ExpressionStatement(ExpressionNode::Switch(_, _, arms)) => {
                for arm in arms {
                    if let dream_syntax::nodes::SwitchArmBody::Block(b) = &arm.body {
                        for s in *b {
                            self.forbid_control_flow_in_defer(s, diagnostics, fallback);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
