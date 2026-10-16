use super::*;

impl Analyzer<'_> {
    pub(super) fn analyze_validation_intrinsic(
        &mut self,
        op: intrinsics::IntrinsicOp,
        method: &SyntaxToken,
        generic_args: &Option<Vec<Type>>,
        params_types: Vec<dream_types::TypeId>,
        arg_hirs: Vec<Option<dream_hir::HExpr>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        let target = generic_args
            .as_ref()
            .and_then(|args| args.first())
            .map(|ty| Self::monomorphize_type(ty, &self.current_generic_bindings))
            .or_else(|| {
                params_types
                    .first()
                    .map(|&ty| self.type_ctx.syntax_type(ty))
            });
        let Some(target) = target else {
            diagnostics.report_error(
                "validation requires a model type argument".to_string(),
                Some(method.position),
            );
            self.hir_none();
            return Ok(Type::Unknown);
        };
        let owner = self.type_ctx.lower(&target);
        let member = match op {
            intrinsics::IntrinsicOp::ValidationDecode => "__validation_decode",
            intrinsics::IntrinsicOp::ValidationValidate => "__validation_validate",
            intrinsics::IntrinsicOp::ValidationSchema => "__validation_schema",
            _ => unreachable!("validation intrinsic dispatch"),
        };
        if self.struct_info(owner).is_none()
            || !self.json_type_encodable(owner)
            || (self.hir_active()
                && self
                    .function_table
                    .method_candidates(owner, member)
                    .is_empty())
        {
            diagnostics.report_error(
                "validation requires an @json model".to_string(),
                Some(method.position),
            );
            self.hir_none();
            return Ok(Type::Unknown);
        }
        let named = |name: &str| {
            let mut token = method.clone();
            token.text = name.to_string();
            Type::from_token(token).unwrap_or(Type::Unknown)
        };
        let result = if op == intrinsics::IntrinsicOp::ValidationSchema {
            named("JsonValue")
        } else {
            let mut token = method.clone();
            token.text = "Result".to_string();
            let ty = Type::Struct(token, Some(vec![target.clone(), named("ValidationErrors")]));
            self.ensure_union_instantiated(
                "Result",
                &[target, named("ValidationErrors")],
                &method.position,
                diagnostics,
            );
            ty
        };
        self.hir_set_type_method_call(owner, member, arg_hirs, &result);
        Ok(result)
    }
}
