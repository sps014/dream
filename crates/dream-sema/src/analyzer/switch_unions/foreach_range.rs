//! Count toward the exclusive end without allocating an iterator. Snapshot both bounds so
//! mutations of a stored range in the body cannot change the traversal.

use super::*;
use crate::errors::SemanticError;
use crate::symbol_table::SymbolTable;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{StatementNode, Type};
use dream_syntax::token::syntax_token::SyntaxToken;
use std::cell::RefCell;
use std::rc::Rc;

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn is_stdlib_range(&self, ty: dream_types::TypeId) -> bool {
        let dream_types::TyKind::Struct(def, args) = self.type_ctx.interner.kind(ty) else {
            return false;
        };
        self.type_ctx.defs.name(*def) == "Range"
            && args.is_empty()
            && self.struct_info(ty).is_some_and(|info| {
                info.file_path
                    .as_deref()
                    .is_some_and(dream_stdlib::is_std_source)
            })
    }

    pub(super) fn analyze_foreach_range(
        &mut self,
        element: &SyntaxToken,
        range_type: &Type,
        iter_hir: Option<dream_hir::HExpr>,
        body: &[StatementNode<'a>],
        ctx: &super::super::AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        use dream_hir::{BinOp, HExpr, HExprKind, HStmt, Overflow};

        let label = self.pending_loop_label.take();
        let scope = Rc::new(RefCell::new(SymbolTable::new(Some(
            ctx.symbol_table.clone(),
        ))));
        ctx.symbol_table.borrow_mut().add_child(scope.clone());
        let int_type = Type::Integer(element.clone());
        if let Err(error) = scope
            .borrow_mut()
            .add_symbol(element.text.clone(), int_type.clone())
        {
            diagnostics.report_error(error.to_string(), Some(element.position));
        }
        let range_local = self.hir_alloc_local("$foreach_range", range_type);
        let index_local = self.hir_alloc_local("$foreach_idx", &int_type);
        let end_local = self.hir_alloc_local("$foreach_end", &int_type);
        let step_local = self.hir_alloc_local("$foreach_step", &int_type);
        let element_local = self.hir_alloc_local(&element.text, &int_type);
        if let (Some(range), Some(index), Some(end), Some(step)) =
            (range_local, index_local, end_local, step_local)
        {
            let range_ty = self.type_ctx.lower(range_type);
            let int = self.type_ctx.interner.int();
            self.hir_assign_local_id(range, iter_hir);
            for (local, name) in [(index, "start"), (end, "end")] {
                let field = self
                    .struct_info(range_ty)
                    .and_then(|info| info.fields.get_index_of(name));
                if let Some(field) = field {
                    self.hir_set_field(Some(self.hx_local(range, range_ty)), field, &int_type);
                    let value = self.hir_take();
                    self.hir_assign_local_id(local, value);
                } else {
                    self.hir_fail();
                }
            }
            let ascending = self.hx_bin(
                BinOp::Le,
                self.hx_local(index, int),
                self.hx_local(end, int),
            );
            let direction = HExpr::new(
                int,
                HExprKind::Ternary {
                    cond: Box::new(ascending),
                    then_expr: Box::new(self.hx_int(1)),
                    else_expr: Box::new(self.hx_int(-1)),
                },
            );
            self.hir_assign_local_id(step, Some(direction));
        }
        self.hir_open_block();
        if let (Some(index), Some(end), Some(step), Some(element)) =
            (index_local, end_local, step_local, element_local)
        {
            let int = self.type_ctx.interner.int();
            let exhausted = self.hx_bin(
                BinOp::Eq,
                self.hx_local(index, int),
                self.hx_local(end, int),
            );
            self.hir_push_stmt(HStmt::If {
                cond: exhausted,
                then_branch: vec![HStmt::Break(None)],
                else_branch: vec![],
            });
            self.hir_assign_local_id(element, Some(self.hx_local(index, int)));
            // Advance before the body so continue cannot skip the step. The end test ensures
            // this addition always moves within the representable bounds.
            let next = HExpr::new(
                int,
                HExprKind::Binary {
                    op: BinOp::Add,
                    lhs: Box::new(self.hx_local(index, int)),
                    rhs: Box::new(self.hx_local(step, int)),
                    overflow: Overflow::Wrapping,
                },
            );
            self.hir_assign_local_id(index, Some(next));
        }
        self.hir_box_captured_binding(&element.text, &int_type);
        self.analyze_body(body, ctx.parent_function, Some(&scope), true, diagnostics)?;
        let body_hir = self.hir_close_block();
        self.hir_while(Some(self.hx_bool(true)), body_hir, label);
        Ok(())
    }
}
