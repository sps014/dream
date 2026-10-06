use super::*;

impl<'a> Analyzer<'a> {
    /// Monomorphizes a method-level generic instance call (`obj.method<T>(args)`). Mirrors
    /// [`analyze_generic_static_method`]: infer/bind type args, register a concrete instance, emit
    /// a `MethodCall` whose `Callee.instance` carries the TypeIds so WASM symbols match the body.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn analyze_generic_instance_method(
        &mut self,
        template: &'a FunctionNode<'a>,
        _base: &str,
        struct_name: &str,
        method: &SyntaxToken,
        generic_args: &Option<Vec<Type>>,
        params: &Vec<ExpressionNode<'a>>,
        ctx: &super::super::super::AnalyzerContext<'a, '_>,
        receiver: Option<dream_hir::HExpr>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        // Explicit type args let us publish monomorphized parameter types as expected types for
        // argument lambdas (`pool.dispatch<int,int>(5, (n) => n + 1)`).
        let early_bindings = if generic_args.as_ref().is_some_and(|g| !g.is_empty()) {
            Some(self.infer_generic_bindings(
                template,
                generic_args,
                &[],
                &method.position,
                diagnostics,
            ))
        } else {
            None
        };

        let expected_params: Option<Vec<Type>> = early_bindings.as_ref().map(|bindings| {
            template
                .parameters
                .iter()
                .skip(1) // implicit `this`
                .map(|p| Self::monomorphize_type(&p.type_, bindings))
                .collect()
        });

        let call_target = format!("{}.{}", struct_name, method.text);
        let saved_call_target = self.current_call_target_name.take();
        self.current_call_target_name = Some(call_target);

        let (mut arg_types, mut arg_hirs, mut arg_is_ref) = self
            .analyze_call_arguments_expecting_ref(
                params,
                expected_params.as_deref(),
                ctx.parent_function,
                ctx.symbol_table,
                diagnostics,
            )?;

        self.current_call_target_name = saved_call_target;

        // Align with the template's parameter list (index 0 is `this`) for inference.
        let owner = receiver
            .as_ref()
            .map(|hir| hir.ty)
            .unwrap_or_else(|| self.type_ctx.lower(&template.parameters[0].type_));
        let mut inference_types = Vec::with_capacity(arg_types.len() + 1);
        inference_types.push(owner);
        inference_types.extend(arg_types.iter().cloned());

        let bindings = early_bindings.unwrap_or_else(|| {
            self.infer_generic_bindings(
                template,
                generic_args,
                &inference_types,
                &method.position,
                diagnostics,
            )
        });

        if !self.member_accessible(
            template.visibility,
            &template.file_path,
            ctx.parent_function.file_path.as_ref(),
            self.in_methods_of(ctx.parent_function, owner),
        ) {
            diagnostics.report_error(
                format!(
                    "'{}' is private to '{}'",
                    method.text,
                    self.type_id_display(owner)
                ),
                Some(method.position),
            );
        }

        self.verify_generic_constraints(
            &template.generic_constraints,
            &bindings,
            &method.position,
            diagnostics,
        );
        self.reject_ref_struct_bindings(&bindings, &method.position, diagnostics);
        let instance = self.register_generic_function_instance(template, &bindings);

        let store_sig = match self.function_table.get_function(&instance) {
            Ok(sig) => sig,
            Err(_) => {
                diagnostics.report_error(
                    format!(
                        "Function '{}' could not be instantiated",
                        template.name.text
                    ),
                    Some(method.position),
                );
                return Ok(Type::Unknown);
            }
        };

        self.check_unsafe_call(&store_sig, method.position, diagnostics);
        self.check_runtime_call(
            &format!("{}.{}", struct_name, method.text),
            store_sig.runtime_support,
            method.position,
            diagnostics,
        );

        let mut expected_params = store_sig.parameters.clone();
        let mut expected_defaults = store_sig.defaults.clone();
        let mut expected_is_ref = store_sig.is_ref.clone();
        if !expected_params.is_empty() {
            expected_params.remove(0);
        }
        if !expected_defaults.is_empty() {
            expected_defaults.remove(0);
        }
        if !expected_is_ref.is_empty() {
            expected_is_ref.remove(0);
        }
        let mut expected_param_tys = store_sig.parameter_types.clone();
        if !expected_param_tys.is_empty() {
            expected_param_tys.remove(0);
        }

        self.pack_variadic_analyzed_args(
            &store_sig,
            &mut arg_types,
            &mut arg_hirs,
            &mut arg_is_ref,
            1,
        );

        self.validate_ref_arguments(
            &format!("method '{}'", method.text),
            &expected_is_ref,
            &arg_is_ref,
            method.position,
            diagnostics,
        );

        let total = expected_params.len();
        let required = Self::required_arg_count(&expected_defaults, total);
        let given = arg_types.len();
        if given < required || given > total {
            let message = if required == total {
                format!(
                    "function {} expects {} parameters, got {}",
                    method.text, total, given
                )
            } else {
                format!(
                    "function {} expects between {} and {} parameters, got {}",
                    method.text, required, total, given
                )
            };
            diagnostics.report_error(message, Some(method.position));
            self.hir_none();
            return Ok(Type::Unknown);
        }

        self.substitute_default_args(
            (&expected_defaults, &expected_param_tys),
            &mut arg_types,
            &mut arg_hirs,
            ctx.parent_function,
            ctx.symbol_table,
            diagnostics,
        )?;

        self.validate_arguments(
            &format!("function {}", method.text),
            &expected_params,
            &arg_types,
            method.position,
            diagnostics,
        );

        let ret_type = Self::async_return_type(
            store_sig.is_async,
            Some(self.type_ctx.syntax_type(store_sig.resolved_return)),
        );
        self.hir_set_method_call(receiver, &store_sig.identity, arg_hirs, &ret_type);
        let call_summary = self.ide_summary(&ret_type);
        self.record_ide_ref(
            method.position,
            ide::IdeTarget::Callee {
                key: store_sig.identity.clone(),
                label: method.text.clone(),
            },
            call_summary,
        );
        Ok(ret_type)
    }
}
