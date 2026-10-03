use super::*;

impl<'a> Analyzer<'a> {
    /// Appends the default values of any omitted trailing parameters to a call's argument lists.
    /// `defaults` is the callee's per-parameter default slice (parallel to its parameters); for each
    /// index at or past the number of supplied arguments that carries a default, its constant
    /// literal is analyzed exactly like an explicit literal argument, extending both `params_types`
    /// (for the per-index type check) and `arg_hirs` (for the emitted call). `param_tys` is the
    /// parallel declared type list so generic unit constructions like `Option.None` can infer.
    /// Callers must have already validated arity (supplied count within `required..=total`).
    pub(crate) fn substitute_default_args(
        &mut self,
        defaults_and_param_tys: (&[Option<Type>], &[Type]),
        params_types: &mut Vec<String>,
        arg_hirs: &mut Vec<Option<dream_hir::HExpr>>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let (defaults, param_tys) = defaults_and_param_tys;
        for i in params_types.len()..defaults.len() {
            if let Some(default) = defaults.get(i).and_then(|d| d.clone()) {
                let saved_expected = self.current_expected_type.take();
                if let Some(ty) = param_tys.get(i) {
                    self.current_expected_type = Some(ty.clone());
                }
                let lit = ExpressionNode::Literal(default);
                let t =
                    self.analyze_expression(&lit, parent_function, symbol_table, diagnostics)?;
                self.current_expected_type = saved_expected;
                arg_hirs.push(self.hir_take());
                params_types.push(t.get_type());
            }
        }
        Ok(())
    }
}
