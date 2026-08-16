//! Infer a generic class's type arguments at a static-method call that omitted `<...>`.

use super::*;
use crate::analyzer::GenericBindings;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::types::is_unknown_type_name;
use dream_syntax::nodes::{ExpressionNode, FunctionNode, LambdaNode, Type};
use indexmap::IndexMap;

impl<'a> Analyzer<'a> {
    /// Resolves `Class.method(args)` when the class is generic and the call wrote no type
    /// arguments. Uses `current_expected_type` when it is `Class<...>` (or `Future<Class<...>>`),
    /// otherwise unifies the chosen static-method formals with the actual arguments.
    ///
    /// Reports a diagnostic and returns `None` when a class parameter cannot be bound (e.g.
    /// `Pair.of(3, 4)` whose `T` does not appear in `of`'s parameters).
    pub(super) fn infer_generic_class_static_type_args(
        &mut self,
        type_name: &str,
        method: &SyntaxToken,
        params: &Vec<ExpressionNode<'a>>,
        ctx: &AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Option<Vec<Type>> {
        let template = *self.generic_structs.get(type_name)?;
        let class_params = template.generic_parameters.as_deref().unwrap_or(&[]);
        if class_params.is_empty() {
            return Some(Vec::new());
        }

        if let Some(args) = self.class_args_from_expected(type_name, class_params.len()) {
            return Some(
                args.into_iter()
                    .map(|t| Self::monomorphize_type(&t, &self.current_generic_bindings))
                    .collect(),
            );
        }

        let mut candidates: Vec<&FunctionNode<'a>> = template
            .methods
            .iter()
            .filter(|m| m.is_static && m.name.text == method.text && m.accessor.is_none())
            .filter(|m| Self::static_arity_matches(m, params.len()))
            .filter(|m| Self::method_lambda_async_compatible(m, params))
            .collect();
        candidates.sort_by_key(|m| Self::future_fun_param_count(m));

        for candidate in candidates {
            if let Some(args) =
                self.try_infer_class_args_from_method(class_params, candidate, params, ctx)
            {
                return Some(args);
            }
        }

        diagnostics.report_error(
            format!(
                "Cannot infer type arguments for generic class '{}'; specify them explicitly, e.g. {}<int>.{}(...)",
                type_name, type_name, method.text
            ),
            Some(method.position),
        );
        None
    }

    fn class_args_from_expected(&self, type_name: &str, nparams: usize) -> Option<Vec<Type>> {
        let expected = self.current_expected_type.as_ref()?;
        Self::class_args_matching(expected, type_name, nparams)
    }

    fn class_args_matching(ty: &Type, type_name: &str, nparams: usize) -> Option<Vec<Type>> {
        match ty {
            Type::Struct(tok, Some(args)) if tok.text == type_name && args.len() == nparams => {
                Some(args.clone())
            }
            Type::Struct(tok, Some(args)) if tok.text == "Future" && args.len() == 1 => {
                Self::class_args_matching(&args[0], type_name, nparams)
            }
            _ => None,
        }
    }

    fn static_arity_matches(method: &FunctionNode<'_>, argc: usize) -> bool {
        let total = method.parameters.len();
        let required = method
            .parameters
            .iter()
            .position(|p| p.default.is_some())
            .unwrap_or(total);
        argc >= required && argc <= total
    }

    fn future_fun_param_count(method: &FunctionNode<'_>) -> usize {
        method
            .parameters
            .iter()
            .filter(|p| Self::is_future_returning_fun(&p.type_))
            .count()
    }

    fn method_lambda_async_compatible(
        method: &FunctionNode<'_>,
        args: &[ExpressionNode<'_>],
    ) -> bool {
        for (i, arg) in args.iter().enumerate() {
            let Some(formal) = method.parameters.get(i) else {
                continue;
            };
            let Type::Function(_, ret) = &formal.type_ else {
                continue;
            };
            let formal_future = Self::future_inner_type(ret).is_some();
            match Self::lambda_is_async(arg) {
                Some(true) if !formal_future => return false,
                Some(false) if formal_future => return false,
                _ => {}
            }
        }
        true
    }

    fn try_infer_class_args_from_method(
        &mut self,
        class_params: &[SyntaxToken],
        method: &FunctionNode<'a>,
        args: &[ExpressionNode<'a>],
        ctx: &AnalyzerContext<'a, '_>,
    ) -> Option<Vec<Type>> {
        let param_names: Vec<String> = class_params.iter().map(|p| p.text.clone()).collect();
        let mut bindings: GenericBindings = IndexMap::new();
        let mut arg_types: Vec<Option<String>> = vec![None; args.len()];

        let paused = self.hir_pause_collection();
        for (i, arg) in args.iter().enumerate() {
            if Self::as_lambda(arg).is_some() {
                continue;
            }
            let mut scratch = DiagnosticBag::new(None);
            let saved_expected = self.current_expected_type.take();
            let ty = self
                .analyze_expression(arg, ctx.parent_function, ctx.symbol_table, &mut scratch)
                .ok();
            let _ = self.hir_take();
            self.current_expected_type = saved_expected;
            if let Some(t) = ty {
                let s = t.get_type();
                if !is_unknown_type_name(&s) {
                    arg_types[i] = Some(s);
                }
            }
        }
        self.hir_resume_collection(paused.0, paused.1);

        Self::bind_class_params_from_args(&param_names, method, &arg_types, &mut bindings);

        for (i, arg) in args.iter().enumerate() {
            let Some(lambda) = Self::as_lambda(arg) else {
                continue;
            };
            let Some(formal) = method.parameters.get(i) else {
                continue;
            };
            let Type::Function(fun_params, _) = &formal.type_ else {
                continue;
            };
            let param_tys: Vec<Type> = if lambda
                .parameters
                .iter()
                .all(|p| !matches!(p.type_, Type::Unknown))
            {
                lambda.parameters.iter().map(|p| p.type_.clone()).collect()
            } else {
                if fun_params.len() != lambda.parameters.len() {
                    continue;
                }
                let substituted: Vec<Type> = fun_params
                    .iter()
                    .map(|p| Self::monomorphize_type(p, &bindings))
                    .collect();
                if substituted
                    .iter()
                    .any(|t| Self::mentions_unbound_class_param(t, &param_names, &bindings))
                {
                    continue;
                }
                substituted
            };
            if param_tys.len() != lambda.parameters.len() {
                continue;
            }
            let mut scratch = DiagnosticBag::new(None);
            let paused = self.hir_pause_collection();
            let Ok(body_ret) = self.infer_lambda_return_type(
                lambda,
                &param_tys,
                ctx.parent_function,
                ctx.symbol_table,
                &mut scratch,
            ) else {
                self.hir_resume_collection(paused.0, paused.1);
                continue;
            };
            self.hir_resume_collection(paused.0, paused.1);
            let ret = if lambda.is_async {
                Self::future_type(body_ret)
            } else {
                body_ret
            };
            let actual = Type::Function(param_tys, Box::new(ret)).get_type();
            arg_types[i] = Some(actual);
        }

        Self::bind_class_params_from_args(&param_names, method, &arg_types, &mut bindings);

        if param_names.iter().any(|p| !bindings.contains_key(p)) {
            return None;
        }
        Some(param_names.iter().map(|p| bindings[p].clone()).collect())
    }

    fn bind_class_params_from_args(
        param_names: &[String],
        method: &FunctionNode<'_>,
        arg_types: &[Option<String>],
        bindings: &mut GenericBindings,
    ) {
        for name in param_names {
            if bindings.contains_key(name) {
                continue;
            }
            let concrete = method
                .parameters
                .iter()
                .enumerate()
                .find_map(|(i, formal)| {
                    arg_types.get(i).and_then(|arg| {
                        arg.as_ref()
                            .and_then(|a| Self::match_generic_type(&formal.type_, a, name))
                    })
                });
            if let Some(concrete) = concrete {
                if !is_unknown_type_name(&concrete) {
                    bindings.insert(name.clone(), Self::concrete_type_from_str(&concrete));
                }
            }
        }
    }

    fn mentions_unbound_class_param(
        ty: &Type,
        param_names: &[String],
        bindings: &GenericBindings,
    ) -> bool {
        match ty {
            Type::Generic(name)
                if param_names.iter().any(|p| p == name) && !bindings.contains_key(name) =>
            {
                true
            }
            Type::Struct(token, None)
                if param_names.iter().any(|p| p == &token.text)
                    && !bindings.contains_key(&token.text) =>
            {
                true
            }
            Type::Array(inner) => Self::mentions_unbound_class_param(inner, param_names, bindings),
            Type::Tuple(elems) => elems
                .iter()
                .any(|e| Self::mentions_unbound_class_param(e, param_names, bindings)),
            Type::Struct(_, Some(args)) => args
                .iter()
                .any(|a| Self::mentions_unbound_class_param(a, param_names, bindings)),
            Type::Function(params, ret) => {
                params
                    .iter()
                    .any(|p| Self::mentions_unbound_class_param(p, param_names, bindings))
                    || Self::mentions_unbound_class_param(ret, param_names, bindings)
            }
            _ => false,
        }
    }

    fn lambda_is_async(expr: &ExpressionNode<'_>) -> Option<bool> {
        match expr {
            ExpressionNode::Lambda(l) => Some(l.is_async),
            ExpressionNode::Parenthesized(_, inner) => Self::lambda_is_async(inner),
            ExpressionNode::NamedArg(_, inner) => Self::lambda_is_async(inner),
            _ => None,
        }
    }

    fn as_lambda<'b>(expr: &'b ExpressionNode<'a>) -> Option<&'a LambdaNode<'a>> {
        match expr {
            ExpressionNode::Lambda(l) => Some(l),
            ExpressionNode::Parenthesized(_, inner) => Self::as_lambda(inner),
            ExpressionNode::NamedArg(_, inner) => Self::as_lambda(inner),
            _ => None,
        }
    }
}
