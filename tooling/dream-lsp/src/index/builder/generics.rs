use super::*;

impl Builder {
    pub(crate) fn class_generic_params(&self, class_name: &str) -> Vec<String> {
        self.decls
            .iter()
            .find(|d| d.name == class_name && matches!(d.kind, SymKind::Class | SymKind::Struct))
            .map(|d| parse_angle_type_args(&d.detail))
            .unwrap_or_default()
    }

    pub(crate) fn infer_method_call_type(
        &self,
        recv: &ExpressionNode,
        method: &SyntaxToken,
        generic_args: &Option<Vec<Type>>,
        args: &[ExpressionNode],
        scope: usize,
    ) -> Option<String> {
        let receiver_ty_opt = self.receiver_type_of(recv, scope);
        let class_name = receiver_ty_opt.as_deref().map(type_base);
        let class_params = class_name
            .map(|n| self.class_generic_params(n))
            .unwrap_or_default();
        let explicit: Vec<String> = generic_args
            .as_ref()
            .map(|g| g.iter().map(|a| a.display_name()).collect())
            .unwrap_or_default();

        let candidates: Vec<&Decl> = self
            .decls
            .iter()
            .filter(|d| {
                d.name == method.text
                    && d.kind == SymKind::Method
                    && class_name.is_some_and(|base| detail_belongs_to(&d.detail, base))
            })
            .collect();
        let candidates = if candidates.is_empty() {
            self.resolve_member_decl(receiver_ty_opt.as_deref(), &method.text)
                .into_iter()
                .collect()
        } else {
            candidates
        };

        let mut best: Option<String> = None;
        for d in candidates {
            let Some(sig) = parse_method_signature(&d.detail) else {
                continue;
            };
            if !sig.accepts_arg_count(args.len()) {
                continue;
            }
            let (formals, ret) = (sig.params, sig.ret);
            if !Self::lambda_async_compatible(&formals, args) {
                continue;
            }
            let method_params = method_generic_param_names(&d.detail);
            let infer_params: &[String] = if !class_params.is_empty() {
                &class_params
            } else {
                &method_params
            };
            let type_args = if !explicit.is_empty() {
                explicit.clone()
            } else if !infer_params.is_empty() {
                match self.infer_class_args_from_call(infer_params, &formals, args, scope) {
                    Some(a) => a,
                    None => continue,
                }
            } else {
                Vec::new()
            };
            if !infer_params.is_empty() && type_args.len() != infer_params.len() {
                continue;
            }
            if type_args.iter().any(|t| t.is_empty()) {
                continue;
            }
            let subst_ret = if infer_params.is_empty() {
                let detail = Index::apply_type_args_to_detail(
                    &d.detail,
                    receiver_ty_opt.as_deref(),
                    &explicit,
                );
                parse_method_signature(&detail)
                    .map(|s| s.ret)
                    .unwrap_or(ret)
            } else {
                substitute_named_type_params(&ret, infer_params, &type_args)
            };
            if infer_params
                .iter()
                .any(|p| type_mentions_param(&subst_ret, p))
            {
                continue;
            }
            let ty = Self::async_call_type(&d.detail, subst_ret);
            let future_score = formals.iter().filter(|f| f.contains("Future<")).count();
            match &best {
                Some(prev) if prev.contains("Future<") && future_score == 0 => {
                    best = Some(ty);
                }
                None => best = Some(ty),
                Some(_) => {}
            }
            if future_score == 0 {
                break;
            }
        }
        best
    }

    pub(crate) fn lambda_async_compatible(formals: &[String], args: &[ExpressionNode]) -> bool {
        for (formal, arg) in formals.iter().zip(args.iter()) {
            let formal_future = split_fun_type_str(formal)
                .map(|(_, ret)| type_base(&ret) == "Future")
                .unwrap_or(false);
            match as_lambda(arg) {
                Some(true) if !formal_future => return false,
                Some(false) if formal_future => return false,
                _ => {}
            }
        }
        true
    }

    pub(crate) fn infer_class_args_from_call(
        &self,
        class_params: &[String],
        formals: &[String],
        args: &[ExpressionNode],
        scope: usize,
    ) -> Option<Vec<String>> {
        let mut actuals: Vec<Option<String>> = vec![None; args.len()];
        for (i, arg) in args.iter().enumerate() {
            if as_lambda(arg).is_some() {
                continue;
            }
            actuals[i] = self.infer_type(arg, scope);
        }
        let mut bindings: Vec<Option<String>> = vec![None; class_params.len()];
        Self::bind_class_params(class_params, formals, &actuals, &mut bindings);

        for (i, arg) in args.iter().enumerate() {
            let Some(is_async) = as_lambda(arg) else {
                continue;
            };
            let Some(lambda) = lambda_node(arg) else {
                continue;
            };
            let Some((fun_params, _)) = split_fun_type_str(&formals[i]) else {
                continue;
            };
            let mut extras: Vec<(String, String)> = Vec::new();
            for (p, fty) in lambda.parameters.iter().zip(fun_params.iter()) {
                let concrete = if !matches!(p.type_, Type::Unknown) {
                    p.type_.display_name()
                } else {
                    let mut t = fty.clone();
                    for (param, bound) in class_params.iter().zip(bindings.iter()) {
                        if let Some(b) = bound {
                            t = substitute_named_type_params(
                                &t,
                                std::slice::from_ref(param),
                                std::slice::from_ref(b),
                            );
                        }
                    }
                    if class_params.iter().any(|p| type_mentions_param(&t, p)) {
                        continue;
                    }
                    t
                };
                extras.push((p.name.text.clone(), concrete));
            }
            if extras.len() != lambda.parameters.len() {
                continue;
            }
            let body_ty = match &lambda.body {
                LambdaBody::Expr(expr) => self.infer_type_with(expr, scope, &extras),
                LambdaBody::Block(_) => None,
            };
            let Some(mut body_ty) = body_ty else {
                continue;
            };
            if is_async && type_base(&body_ty) != "Future" {
                body_ty = format!("Future<{body_ty}>");
            }
            let fun_ty = format!(
                "fun({}): {}",
                extras
                    .iter()
                    .map(|(_, t)| t.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                body_ty
            );
            actuals[i] = Some(fun_ty);
        }
        Self::bind_class_params(class_params, formals, &actuals, &mut bindings);
        if bindings.iter().any(|b| b.is_none()) {
            return None;
        }
        Some(bindings.into_iter().map(|b| b.unwrap()).collect())
    }

    pub(crate) fn bind_class_params(
        class_params: &[String],
        formals: &[String],
        actuals: &[Option<String>],
        bindings: &mut [Option<String>],
    ) {
        for (i, param) in class_params.iter().enumerate() {
            if bindings[i].is_some() {
                continue;
            }
            for (formal, actual) in formals.iter().zip(actuals.iter()) {
                let Some(actual) = actual else {
                    continue;
                };
                if let Some(c) = unify_type_param(formal, actual, param) {
                    bindings[i] = Some(c);
                    break;
                }
            }
        }
    }
}
