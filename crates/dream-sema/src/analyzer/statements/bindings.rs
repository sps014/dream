//! `let` declarations, simple `name = value` assignments, and `return`.

use super::*;
use dream_diagnostics::DiagnosticBag;
use crate::errors::SemanticError;
use crate::symbol_table::SymbolTable;
use dream_syntax::nodes::{ExpressionNode, FunctionNode, Type};
use dream_syntax::token::syntax_token::SyntaxToken;
use std::cell::RefCell;
use std::rc::Rc;

impl<'a> Analyzer<'a> {
    /// C#/Rust-style discard binding: `let _ = …` / tuple `_` never enters the symbol table.
    pub(in crate::analyzer) fn is_discard_binding(name: &str) -> bool {
        name == "_"
    }

    /// Binds a local, or evaluates+drops it when `name` is the discard `_`.
    fn bind_or_discard_local(
        &mut self,
        name: &SyntaxToken,
        var_type: Type,
        value: Option<dream_hir::HExpr>,
        is_const: bool,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) {
        if Self::is_discard_binding(&name.text) {
            // Generic function items are not runtime values — nothing to drop.
            if !matches!(var_type, Type::GenericFunctionItem(_)) {
                self.hir_expr_stmt(value);
            }
            return;
        }
        if matches!(var_type, Type::GenericFunctionItem(_)) {
            if let Err(e) = (*symbol_table)
                .as_ref()
                .borrow_mut()
                .add_symbol(name.text.clone(), var_type)
            {
                diagnostics.report_error(e.to_string(), Some(name.position));
            }
            if is_const {
                (*symbol_table)
                    .as_ref()
                    .borrow_mut()
                    .mark_const(name.text.clone());
            }
            return;
        }
        self.record_capturing_fun_local(&name.text, &var_type, value.as_ref());
        self.hir_declare_local(&name.text, &var_type, value);
        {
            let mut st = (*symbol_table).as_ref().borrow_mut();
            if let Err(e) = st.add_symbol(name.text.clone(), var_type) {
                drop(st);
                diagnostics.report_error(e.to_string(), Some(name.position));
            } else {
                st.track_local(name.text.clone(), name.position);
                if is_const {
                    st.mark_const(name.text.clone());
                }
            }
        }
    }

    pub(in crate::analyzer) fn analyze_declaration(
        &mut self,
        left: &SyntaxToken,
        type_annotation: &Option<Type>,
        right: &ExpressionNode<'a>,
        is_const: bool,
        ctx: &super::super::AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        if !Self::is_discard_binding(&left.text) {
            self.check_reserved_name(left, "variable", diagnostics);
        }
        // Inside a monomorphized generic body, substitute the type parameters in the annotation with
        // their concrete types (e.g. `let cmp: fun(T, T): int` becomes `fun(int, int): int`), so the
        // published expected type, the initializer check, and the recorded variable type are all
        // concrete. Outside a generic body the bindings are empty and this just clones.
        let mono_annotation = type_annotation
            .as_ref()
            .map(|t| Self::monomorphize_type(t, &self.current_generic_bindings));
        let type_annotation = &mono_annotation;
        // Empty array literals carry no element type, so the declaration must supply one via an
        // array-typed (`int[]`) or `List<T>` annotation (e.g. `let xs: int[] = [];` / `let xs:
        // List<int> = [];`). With a valid annotation the literal is handled on the normal path
        // below (the annotation is published as the expected type, which the array-literal
        // analysis uses to allocate a zero-length array, or lower to `List<T>.from_array([])`).
        if let ExpressionNode::ArrayLiteral(_, elements) = right {
            if elements.is_empty()
                && !type_annotation.as_ref().is_some_and(|t| {
                    t.is_array() || Self::collection_generic_arg(t, "List").is_some()
                })
            {
                self.hir_fail();
                diagnostics.report_error(
                    "cannot infer the element type of an empty array literal; add an array type annotation, e.g. `let xs: int[] = [];`".to_string(),
                    Some(left.position),
                );
                return Ok(());
            }
        }
        //return right type. A type annotation is published as the expected type so a generic
        // union's nullary variant (`let o: Option<int> = Option.None;`) can resolve its arguments.
        let saved_expected = self.current_expected_type.take();
        self.current_expected_type = type_annotation.clone();
        // Recover at the binding site: even when the initializer short-circuits, fall back to the
        // poison type so the variable is still registered (with its annotated type, if any) and
        // later uses of it don't spuriously report "does not exist".
        let right_type = self
            .analyze_expression(right, ctx.parent_function, ctx.symbol_table, diagnostics)
            .unwrap_or(Type::Unknown);
        let mut value = self.hir_take();
        self.current_expected_type = saved_expected;

        let var_type = if let Some(t) = type_annotation {
            // A user-defined `@cast("implicit")` conversion is tried before the built-in
            // assignability check, so `let x: T = expr;` accepts it exactly like numeric widening.
            let converted_type;
            (converted_type, value) = self.apply_implicit_cast(&right_type, t, value);
            self.compare_data_type(t, &converted_type, &left.position, diagnostics)?;
            t.clone()
        } else {
            right_type.clone()
        };

        self.bind_or_discard_local(
            left,
            var_type,
            value,
            is_const,
            ctx.symbol_table,
            diagnostics,
        );
        if !Self::is_discard_binding(&left.text) {
            self.unmove_local(&left.text);
            self.hir_flush_ref_writebacks();
        }
        Ok(())
    }

    /// `let (a, b) = expr;` — positional tuple destructure, including nested `(a, (b, c))`.
    pub(in crate::analyzer) fn analyze_tuple_declaration(
        &mut self,
        pattern: &dream_syntax::nodes::PatternNode,
        type_annotation: &Option<Type>,
        right: &ExpressionNode<'a>,
        is_const: bool,
        ctx: &super::super::AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        for name in pattern.binding_names() {
            self.check_reserved_name(name, "variable", diagnostics);
        }
        let mono_annotation = type_annotation
            .as_ref()
            .map(|t| Self::monomorphize_type(t, &self.current_generic_bindings));
        self.bind_destructure_pattern(
            pattern,
            right,
            mono_annotation.as_ref(),
            is_const,
            ctx,
            diagnostics,
        )?;
        self.hir_flush_ref_writebacks();
        Ok(())
    }

    fn bind_destructure_pattern(
        &mut self,
        pattern: &dream_syntax::nodes::PatternNode,
        expr: &ExpressionNode<'a>,
        expected: Option<&Type>,
        is_const: bool,
        ctx: &super::super::AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        use dream_syntax::nodes::PatternNode;
        match pattern {
            PatternNode::Tuple(pats) => {
                if let ExpressionNode::TupleLiteral(_, elems) = expr {
                    if elems.len() == pats.len() {
                        let expected_elems: Option<Vec<Type>> = match expected {
                            Some(Type::Tuple(ts)) if ts.len() == elems.len() => Some(ts.clone()),
                            _ => None,
                        };
                        for (i, (p, e)) in pats.iter().zip(elems.iter()).enumerate() {
                            let slot_ty = expected_elems.as_ref().map(|es| &es[i]);
                            self.bind_destructure_pattern(
                                p, e, slot_ty, is_const, ctx, diagnostics,
                            )?;
                        }
                        return Ok(());
                    }
                }
                let saved_expected = self.current_expected_type.take();
                self.current_expected_type = expected.cloned();
                let right_type = self
                    .analyze_expression(expr, ctx.parent_function, ctx.symbol_table, diagnostics)
                    .unwrap_or(Type::Unknown);
                let mut value = self.hir_take();
                self.current_expected_type = saved_expected;
                let tuple_ty = if let Some(t) = expected {
                    let converted;
                    (converted, value) = self.apply_implicit_cast(&right_type, t, value);
                    self.compare_data_type(
                        t,
                        &converted,
                        &pattern.position().unwrap_or_else(empty_span),
                        diagnostics,
                    )?;
                    t.clone()
                } else {
                    right_type.clone()
                };
                self.bind_pattern_from_value(
                    pattern,
                    &tuple_ty,
                    value,
                    is_const,
                    ctx.symbol_table,
                    diagnostics,
                );
                Ok(())
            }
            PatternNode::Binding(name) | PatternNode::Wildcard(name) => {
                let saved = self.current_expected_type.take();
                self.current_expected_type = expected.cloned();
                let elem_ty = self
                    .analyze_expression(expr, ctx.parent_function, ctx.symbol_table, diagnostics)
                    .unwrap_or(Type::Unknown);
                let mut value = self.hir_take();
                self.current_expected_type = saved;
                let var_type = if let Some(t) = expected {
                    let converted;
                    (converted, value) = self.apply_implicit_cast(&elem_ty, t, value);
                    self.compare_data_type(t, &converted, &name.position, diagnostics)?;
                    t.clone()
                } else {
                    elem_ty
                };
                self.bind_or_discard_local(
                    name,
                    var_type,
                    value,
                    is_const,
                    ctx.symbol_table,
                    diagnostics,
                );
                Ok(())
            }
            _ => {
                diagnostics.report_error(
                    "let/const destructure only allows names, '_' and nested tuples".to_string(),
                    pattern.position(),
                );
                Ok(())
            }
        }
    }

    fn bind_pattern_from_value(
        &mut self,
        pattern: &dream_syntax::nodes::PatternNode,
        ty: &Type,
        value: Option<dream_hir::HExpr>,
        is_const: bool,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) {
        use dream_syntax::nodes::PatternNode;
        match pattern {
            PatternNode::Binding(name) | PatternNode::Wildcard(name) => {
                self.bind_or_discard_local(name, ty.clone(), value, is_const, symbol_table, diagnostics);
            }
            PatternNode::Tuple(pats) => {
                let Type::Tuple(elem_tys) = ty else {
                    if !ty.is_unknown() {
                        diagnostics.report_error(
                            format!("cannot destructure non-tuple type {}", ty.display_name()),
                            pattern.position(),
                        );
                    }
                    self.hir_fail();
                    return;
                };
                if elem_tys.len() != pats.len() {
                    diagnostics.report_error(
                        format!(
                            "tuple has {} elements but destructuring binds {}",
                            elem_tys.len(),
                            pats.len()
                        ),
                        pattern.position(),
                    );
                    self.hir_fail();
                    return;
                }
                let pos = pattern.position().map(|s| s.start).unwrap_or(0);
                let temp_name = format!("__tuple_tmp_{}", pos);
                self.hir_declare_local(&temp_name, ty, value);
                if let Err(e) = (*symbol_table)
                    .as_ref()
                    .borrow_mut()
                    .add_symbol(temp_name.clone(), ty.clone())
                {
                    diagnostics.report_error(e.to_string(), pattern.position());
                }
                for (i, p) in pats.iter().enumerate() {
                    let elem_ty = elem_tys[i].clone();
                    self.hir_set_var(&temp_name);
                    let base = self.hir_take();
                    self.hir_set_field(base, i, &elem_ty);
                    let field_val = self.hir_take();
                    self.bind_pattern_from_value(
                        p,
                        &elem_ty,
                        field_val,
                        is_const,
                        symbol_table,
                        diagnostics,
                    );
                }
            }
            _ => {
                diagnostics.report_error(
                    "let/const destructure only allows names, '_' and nested tuples".to_string(),
                    pattern.position(),
                );
            }
        }
    }

    pub(in crate::analyzer) fn analyze_assignment(
        &mut self,
        left: &SyntaxToken,
        right: &ExpressionNode<'a>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        if Self::is_discard_binding(&left.text) {
            diagnostics.report_error(
                "'_' is a discard and cannot be assigned to".to_string(),
                Some(left.position),
            );
            self.hir_fail();
            return Ok(());
        }
        if (*symbol_table).as_ref().borrow().is_const(&left.text) {
            diagnostics.report_error(
                format!(
                    "Cannot assign to '{}' because it is a const binding",
                    left.text
                ),
                Some(left.position),
            );
        }
        // Peek the target's declared type first so it can drive inference of the right-hand side
        // (e.g. an untyped empty array literal `xs = []` resolves to the variable's element type).
        let l = match (*symbol_table).as_ref().borrow().get_symbol(left) {
            Ok(sym) => sym,
            Err(e) => {
                diagnostics.report_error(e.to_string(), Some(left.position));
                self.hir_fail();
                return Ok(());
            }
        };
        let saved_expected = self.current_expected_type.take();
        self.current_expected_type = Some(l.clone());
        let r = self
            .analyze_expression(right, parent_function, symbol_table, diagnostics)
            .unwrap_or(Type::Unknown);
        let value = self.hir_take();
        self.current_expected_type = saved_expected;
        self.compare_data_type(&l, &r, &left.position, diagnostics)?;
        self.record_capturing_fun_local(&left.text, &l, value.as_ref());
        self.hir_assign_local(&left.text, value);
        self.unmove_local(&left.text);
        Ok(())
    }
    pub(in crate::analyzer) fn analyze_return(
        &mut self,
        expression: &Option<ExpressionNode<'a>>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        match (expression, &parent_function.return_type) {
            (Some(expression), Some(return_type)) => {
                let saved_expected = self.current_expected_type.take();
                self.current_expected_type = Some(return_type.clone());
                let r = self
                    .analyze_expression(expression, parent_function, symbol_table, diagnostics)
                    .unwrap_or(Type::Unknown);
                let value = self.hir_take();
                self.current_expected_type = saved_expected;
                self.compare_data_type(
                    return_type,
                    &r,
                    &parent_function.name.position,
                    diagnostics,
                )?;
                let target = self.type_ctx.lower(return_type);
                self.hir_return_value(value, Some(target));
            }
            // A bare `return;` is allowed in a `void` function (an explicit `: void` annotation
            // parses to `Some(Type::Void)`, which is semantically the same as an unannotated
            // function); it exits early with no value.
            (None, Some(Type::Void)) => self.hir_return_void(),
            (None, &Some(_)) => {
                self.hir_fail();
                diagnostics.report_error(
                    format!(
                        "return type mismatch at  {}",
                        parent_function.name.position.get_point_str()
                    ),
                    Some(parent_function.name.position),
                );
            }
            (Some(_), &None) => {
                self.hir_fail();
                diagnostics.report_error(
                    format!(
                        "return type mismatch at {}",
                        parent_function.name.position.get_point_str()
                    ),
                    Some(parent_function.name.position),
                );
            }
            (None, &None) => self.hir_return_void(),
        };
        Ok(())
    }
}
