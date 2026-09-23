//! `if`/`else if`/`else` analysis, including the compile-time `is` fold that prunes dead branches
//! and the `is`-with-binding flow typing declared into the taken branch.

use super::*;
use crate::errors::SemanticError;
use dream_diagnostics::DiagnosticBag;
use dream_hir::{HExpr, HStmt};
use dream_syntax::nodes::{ExpressionNode, StatementNode, Type};
use dream_syntax::token::syntax_token::SyntaxToken;

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn analyze_if_else(
        &mut self,
        statement: &StatementNode<'a>,
        ctx: &super::super::AnalyzerContext<'a, '_>,
        has_parent_while: bool,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let StatementNode::IfElse(condition, if_body, else_if, else_body) = statement else {
            unreachable!()
        };
        // Live branches of the chain in source order, each `(condition HIR, body)`. An `is` condition
        // on a concrete (non-`object`) operand folds to a compile-time constant: a `false` branch is
        // dead and emits no HIR, and a `true` branch is unconditionally taken, becoming the terminal
        // `else` and ending the chain. Dead branches are still type-checked outside generic
        // instantiations (see `check_dead_branch`). Regular conditions keep their HIR.
        let mut arms: Vec<(HExpr, Vec<HStmt>)> = Vec::new();
        let mut terminal: Vec<HStmt> = Vec::new();

        // Every branch of the chain (primary, then each `else if`) as `(condition, position, body)`.
        let branches = std::iter::once((condition, condition.position(), if_body))
            .chain(else_if.iter().map(|i| (&i.0, i.0.position(), &i.1)));

        let mut branch_moved: Vec<std::collections::HashSet<String>> = Vec::new();
        let before_if = self.snapshot_moved();

        let mut taken_index: Option<usize> = None;
        for (index, (cond_expr, cond_pos, body)) in branches.enumerate() {
            self.restore_moved(before_if.clone());
            // An `is`-with-binding condition declares a narrowed local `name: T` scoped to the taken
            // branch only. This covers a bare `if (x is T name)` and every `is`-binding reachable
            // through a top-level `&&` chain (`if (a && x is T name)`), each of which is guaranteed to
            // hold in the then-branch. Both the compile-time fold and the runtime path introduce them
            // into that branch's scope.
            let mut bindings: Vec<(&SyntaxToken, &Type, &ExpressionNode<'a>)> = Vec::new();
            Self::collect_is_bindings(cond_expr, &mut bindings);

            // `is` fold: an operand with a concrete (non-`object`, non-interface) static type resolves
            // at compile time, so a branch is either taken unconditionally or is dead. An `object` or
            // interface operand needs a runtime tag check, so it falls through to the general
            // (runtime-`IsType`) path below.
            let mut folded = cond_expr;
            while let ExpressionNode::Parenthesized(_, inner) = folded {
                folded = inner;
            }
            if let ExpressionNode::IsExpression(left, right_type, _) = folded {
                let left_t = self
                    .analyze_expression(left, ctx.parent_function, ctx.symbol_table, diagnostics)
                    .unwrap_or(Type::Unknown);
                let left_name = left_t.get_type();
                let runtime =
                    left_t.is_object() || left_t.is_unknown() || self.is_interface_name(&left_name);
                if !runtime {
                    let left_id = self.type_ctx.lower(&left_t);
                    let right_id = self.type_ctx.lower(right_type);
                    if left_id == right_id {
                        let branch_scope = self.branch_scope(ctx.symbol_table);
                        self.hir_open_block();
                        self.declare_is_bindings(&bindings, &branch_scope, ctx, diagnostics)?;
                        self.analyze_body(
                            body,
                            ctx.parent_function,
                            Some(&branch_scope),
                            has_parent_while,
                            diagnostics,
                        )?;
                        terminal = self.hir_close_block();
                        branch_moved.push(self.snapshot_moved());
                        taken_index = Some(index);
                        break;
                    } else {
                        self.check_dead_branch(
                            None,
                            &bindings,
                            body,
                            ctx,
                            has_parent_while,
                            diagnostics,
                        )?;
                        continue;
                    }
                }
            }

            let cond_type = self
                .analyze_expression(
                    cond_expr,
                    ctx.parent_function,
                    ctx.symbol_table,
                    diagnostics,
                )
                .unwrap_or(Type::Unknown);
            let cond_hir = self.hir_take();
            self.check_bool_condition("if", &cond_type, cond_pos, diagnostics);
            let branch_scope = self.branch_scope(ctx.symbol_table);
            self.hir_open_block();
            self.declare_is_bindings(&bindings, &branch_scope, ctx, diagnostics)?;
            self.analyze_body(
                body,
                ctx.parent_function,
                Some(&branch_scope),
                has_parent_while,
                diagnostics,
            )?;
            let body_hir = self.hir_close_block();
            branch_moved.push(self.snapshot_moved());
            match cond_hir {
                Some(cond_hir) => arms.push((cond_hir, body_hir)),
                None => self.hir_fail(),
            }
        }

        if let Some(taken) = taken_index {
            let rest = std::iter::once((condition, if_body))
                .chain(else_if.iter().map(|i| (&i.0, &i.1)))
                .skip(taken + 1);
            for (cond_expr, body) in rest {
                let mut bindings = Vec::new();
                Self::collect_is_bindings(cond_expr, &mut bindings);
                self.check_dead_branch(
                    Some(cond_expr),
                    &bindings,
                    body,
                    ctx,
                    has_parent_while,
                    diagnostics,
                )?;
            }
            if let Some(body) = else_body {
                self.check_dead_branch(None, &[], body, ctx, has_parent_while, diagnostics)?;
            }
        } else {
            self.restore_moved(before_if.clone());
            if let Some(body) = else_body {
                self.hir_open_block();
                self.analyze_body(
                    body,
                    ctx.parent_function,
                    Some(ctx.symbol_table),
                    has_parent_while,
                    diagnostics,
                )?;
                terminal = self.hir_close_block();
                branch_moved.push(self.snapshot_moved());
            } else {
                branch_moved.push(before_if.clone());
            }
        }

        let mut merged = before_if;
        for m in branch_moved {
            merged = merged.union(&m).cloned().collect();
        }
        self.restore_moved(merged);

        // Fold the live arms (innermost last) into a single nested `if`/`else` and emit it.
        let mut chain = terminal;
        for (cond, body) in arms.into_iter().rev() {
            chain = vec![dream_hir::HStmt::If {
                cond,
                then_branch: body,
                else_branch: chain,
            }];
        }
        for stmt in chain {
            self.hir_push_stmt(stmt);
        }
        Ok(())
    }

    /// Type-checks a branch the `is` fold proved dead, without emitting HIR for it. Inside a generic
    /// instantiation the branch is skipped instead: it is typically only valid for *other* type
    /// arguments (`if (x is int) { x + 1 }` under `T = string`), so checking it would report errors
    /// the author cannot fix. Narrowed `is` bindings are declared without the cast validation the
    /// live path performs, because the cast is exactly what the fold proved impossible.
    fn check_dead_branch(
        &mut self,
        cond: Option<&ExpressionNode<'a>>,
        bindings: &[(&SyntaxToken, &Type, &ExpressionNode<'a>)],
        body: &[StatementNode<'a>],
        ctx: &super::super::AnalyzerContext<'a, '_>,
        has_parent_while: bool,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        if !self.current_generic_bindings.is_empty()
            || ctx.parent_function.generic_parameters.is_some()
        {
            return Ok(());
        }
        let moved = self.snapshot_moved();
        let (collecting, ok) = self.hir_pause_collection();
        let result = (|| {
            if let Some(cond) = cond {
                self.analyze_expression(cond, ctx.parent_function, ctx.symbol_table, diagnostics)?;
            }
            let branch_scope = self.branch_scope(ctx.symbol_table);
            for &(name, target_ty, _) in bindings {
                self.check_reserved_name(name, "variable", diagnostics);
                if let Err(e) = (*branch_scope)
                    .borrow_mut()
                    .add_symbol(name.text.clone(), target_ty.clone())
                {
                    diagnostics.report_error(e.to_string(), Some(name.position));
                }
            }
            self.analyze_body(
                body,
                ctx.parent_function,
                Some(&branch_scope),
                has_parent_while,
                diagnostics,
            )
        })();
        self.hir_resume_collection(collecting, ok);
        self.restore_moved(moved);
        result
    }
}
