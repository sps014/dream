use super::*;

impl<'a> Analyzer<'a> {
    /// Analyzes `callee(args)` where `callee` is an arbitrary expression (postfix call). Only
    /// `fun(...)` values (and `js`-typed values) are callable this way; free-function / constructor
    /// lookup stays on the named [`analyze_function_call`] path.
    pub(crate) fn analyze_expr_call(
        &mut self,
        callee: &ExpressionNode<'a>,
        generic_args: &Option<Vec<Type>>,
        params: &Vec<ExpressionNode<'a>>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        if generic_args
            .as_ref()
            .map(|g| !g.is_empty())
            .unwrap_or(false)
            && let Some(name) = unwrap_callee_ident(callee)
        {
            return self.analyze_function_call(
                name,
                generic_args,
                params,
                parent_function,
                symbol_table,
                diagnostics,
            );
        }

        let callee_ty =
            self.analyze_expression(callee, parent_function, symbol_table, diagnostics)?;
        let callee_hir = self.hir_take();
        let span = callee.position();

        if generic_args
            .as_ref()
            .map(|g| !g.is_empty())
            .unwrap_or(false)
        {
            if let Type::GenericFunctionItem(gname) = &callee_ty {
                let tok = SyntaxToken::new(
                    TokenKind::IdentifierToken,
                    span.unwrap_or_else(empty_span),
                    gname.clone(),
                );
                return self.analyze_function_call(
                    &tok,
                    generic_args,
                    params,
                    parent_function,
                    symbol_table,
                    diagnostics,
                );
            }
            diagnostics.report_error(
                "type arguments are not valid on a non-generic function value".to_string(),
                span,
            );
        }

        let mut arg_hirs = Vec::with_capacity(params.len());
        let mut params_types = Vec::with_capacity(params.len());
        let mut arg_is_ref = Vec::with_capacity(params.len());
        for param in params.iter() {
            if let ExpressionNode::RefArgument(_, inner) = param {
                arg_is_ref.push(true);
                match self.analyze_ref_argument(inner, parent_function, symbol_table, diagnostics) {
                    Some((t, hir)) => {
                        arg_hirs.push(hir);
                        params_types.push(self.type_ctx.lower(&t));
                    }
                    None => {
                        arg_hirs.push(None);
                        params_types.push(self.type_ctx.interner.error());
                    }
                }
                continue;
            }
            arg_is_ref.push(false);
            let t = self.analyze_expression(param, parent_function, symbol_table, diagnostics)?;
            arg_hirs.push(self.hir_take());
            params_types.push(self.type_ctx.lower(&t));
        }

        if self.is_js_type(&callee_ty) {
            let recv = callee_hir;
            self.desugar_js_invoke(recv, arg_hirs, span, diagnostics);
            return Ok(Self::js_type());
        }

        if let Type::Function(param_types, ret) = &callee_ty {
            if param_types.len() != params_types.len() {
                diagnostics.report_error(
                    format!(
                        "function value expects {} arguments, got {}",
                        param_types.len(),
                        params_types.len()
                    ),
                    span,
                );
                self.hir_none();
                return Ok((**ret).clone());
            }
            let expected_is_ref: Vec<bool> = param_types
                .iter()
                .map(|t| Self::peel_ref_box(t).1)
                .collect();
            self.validate_ref_arguments(
                "function value",
                &expected_is_ref,
                &arg_is_ref,
                span.unwrap_or_else(empty_span),
                diagnostics,
            );
            let expected_strs: Vec<dream_types::TypeId> = param_types
                .iter()
                .map(|t| self.type_ctx.lower(&Self::peel_ref_box(t).0))
                .collect();
            self.validate_arguments(
                "function value",
                &expected_strs,
                &params_types,
                span.unwrap_or_else(empty_span),
                diagnostics,
            );
            match callee_hir {
                Some(boxed) => self.hir_set_indirect_call_expr(boxed, arg_hirs, ret.as_ref()),
                None => self.hir_none(),
            }
            return Ok((**ret).clone());
        }

        if callee_ty.is_unknown() {
            self.hir_none();
            return Ok(Type::Unknown);
        }

        Err(report(
            diagnostics,
            format!(
                "cannot call value of type '{}'",
                self.ty_display(&callee_ty)
            ),
            span,
        ))
    }
}
