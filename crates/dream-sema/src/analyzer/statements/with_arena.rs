//! `with ArenaAllocator()` / `with ArenaAllocator(nbytes) { body }`.

use super::*;
use dream_hir::{HExpr, HExprKind};
use dream_syntax::nodes::{ExpressionNode, StatementNode};

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn analyze_with_arena(
        &mut self,
        setup: &ExpressionNode<'a>,
        body: &[StatementNode<'a>],
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        has_parent_while: bool,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let (ok, size_expr) = match setup {
            ExpressionNode::FunctionCall(name, _, args) if name.text == "ArenaAllocator" => {
                if args.len() > 1 {
                    diagnostics.report_error(
                        "'ArenaAllocator' takes at most one byte-size argument".to_string(),
                        Some(name.position),
                    );
                }
                let size = if let Some(arg) = args.first() {
                    let ty = self
                        .analyze_expression(arg, parent_function, symbol_table, diagnostics)
                        .unwrap_or(Type::Unknown);
                    if !ty.is_unknown() && !ty.is_int() {
                        diagnostics.report_error(
                            format!(
                                "'ArenaAllocator' size must be 'int', got '{}'",
                                ty.get_type()
                            ),
                            arg.position(),
                        );
                    }
                    self.hir_take()
                } else {
                    self.hir_set_last(Some(HExpr::new(
                        self.type_ctx.interner.int(),
                        HExprKind::IntLit(65536),
                    )));
                    self.hir_take()
                };
                (true, size)
            }
            _ => {
                diagnostics.report_error(
                    "'with' requires 'ArenaAllocator()' or 'ArenaAllocator(nbytes)'".to_string(),
                    setup.position(),
                );
                (false, None)
            }
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
        if ok {
            self.hir_with_arena(size_expr, body_hir);
        }
        Ok(())
    }
}
