use super::*;

impl Builder {
    pub(crate) fn owner_matches(&self, owner: &str, receiver: TypeId) -> bool {
        let ctx = self.type_ctx.borrow();
        match ctx.interner.kind(receiver) {
            TyKind::Struct(def, _) => ctx.resolve(DefKind::Struct, owner) == Some(*def),
            TyKind::Union(def, _) => ctx.resolve(DefKind::Union, owner) == Some(*def),
            TyKind::Interface(def, _) => ctx.resolve(DefKind::Interface, owner) == Some(*def),
            TyKind::Enum(def) => ctx.resolve(DefKind::Enum, owner) == Some(*def),
            TyKind::Prim(prim) => owner == prim.name(),
            TyKind::Js => owner == "js",
            TyKind::Object => owner == "object",
            TyKind::Array(_) => owner.ends_with("[]"),
            _ => false,
        }
    }
    pub(crate) fn receiver_bindings(&self, receiver: TypeId) -> IndexMap<String, TypeId> {
        let ctx = self.type_ctx.borrow();
        match ctx.interner.kind(receiver) {
            TyKind::Struct(def, args) | TyKind::Union(def, args) | TyKind::Interface(def, args) => {
                ctx.defs
                    .get(*def)
                    .generic_params
                    .iter()
                    .cloned()
                    .zip(args.iter().copied())
                    .collect()
            }
            _ => IndexMap::new(),
        }
    }

    pub(crate) fn infer_call(
        &self,
        callable: &Callable,
        receiver: Option<TypeId>,
        explicit: &[Type],
        args: &[ExpressionNode],
        scope: usize,
        extras: &[(String, TypeId)],
    ) -> Option<TypeId> {
        let required = callable
            .params
            .iter()
            .filter(|p| p.default.is_none() && !p.is_variadic)
            .count();
        let variadic = callable.params.last().is_some_and(|p| p.is_variadic);
        if args.len() < required || (!variadic && args.len() > callable.params.len()) {
            return None;
        }
        let mut bindings = receiver
            .map(|r| self.receiver_bindings(r))
            .unwrap_or_default();
        let owner_params = callable
            .owner
            .as_ref()
            .and_then(|name| {
                let ctx = self.type_ctx.borrow();
                let def = ctx
                    .resolve(DefKind::Struct, name)
                    .or_else(|| ctx.resolve(DefKind::Union, name))?;
                Some(ctx.defs.get(def).generic_params.clone())
            })
            .unwrap_or_default();
        let infer_params = if callable.generics.is_empty() {
            &owner_params
        } else {
            &callable.generics
        };
        if !explicit.is_empty() {
            if explicit.len() != infer_params.len() {
                return None;
            }
            let mut ctx = self.type_ctx.borrow_mut();
            for (name, ty) in infer_params.iter().zip(explicit) {
                bindings.insert(name.clone(), ctx.lower(ty));
            }
        }
        let all_params: Vec<_> = owner_params
            .iter()
            .chain(callable.generics.iter())
            .cloned()
            .collect();
        for (formal, arg) in callable.params.iter().zip(args) {
            if as_lambda(arg).is_some() {
                continue;
            }
            if let Some(actual) = self.infer_id(arg, scope, extras)
                && !self.bind_type(&formal.type_, actual, &all_params, &mut bindings) {
                    return None;
                }
        }
        for (formal, arg) in callable.params.iter().zip(args) {
            let Some(lambda) = lambda_node(arg) else {
                continue;
            };
            let Type::Function(params, ret) = &formal.type_ else {
                return None;
            };
            if params.len() != lambda.parameters.len() {
                return None;
            }
            let expects_future = matches!(ret.as_ref(), Type::Struct(token, _) if token.text == dream::syntax::nodes::types::FUTURE_TYPE);
            if lambda.is_async != expects_future {
                return None;
            }
            let mut lambda_extras = extras.to_vec();
            let mut actual_params = Vec::new();
            for (param, formal_ty) in lambda.parameters.iter().zip(params) {
                let id = self.type_ctx.borrow_mut().lower_with(
                    if param.type_.is_unknown() {
                        formal_ty
                    } else {
                        &param.type_
                    },
                    &bindings,
                );
                if matches!(self.type_ctx.borrow().interner.kind(id), TyKind::Error) {
                    return None;
                }
                lambda_extras.push((param.name.text.clone(), id));
                actual_params.push(id);
            }
            let LambdaBody::Expr(body) = &lambda.body else {
                return None;
            };
            let result = self.infer_id(body, scope, &lambda_extras)?;
            let result = call_result(&mut self.type_ctx.borrow_mut(), lambda.is_async, result)?;
            let actual = self
                .type_ctx
                .borrow_mut()
                .interner
                .func(actual_params, result);
            if !self.bind_type(&formal.type_, actual, &all_params, &mut bindings) {
                return None;
            }
        }
        if all_params.iter().any(|name| !bindings.contains_key(name)) {
            return None;
        }
        let mut ctx = self.type_ctx.borrow_mut();
        let ret = ctx.lower_with(&callable.ret, &bindings);
        if matches!(ctx.interner.kind(ret), TyKind::Error) {
            return None;
        }
        call_result(&mut ctx, callable.is_async, ret)
    }

    fn bind_type(
        &self,
        formal: &Type,
        actual: TypeId,
        params: &[String],
        bindings: &mut IndexMap<String, TypeId>,
    ) -> bool {
        let name = match formal {
            Type::Generic(name) => Some(name.as_str()),
            Type::Struct(token, None) => Some(token.text.as_str()),
            _ => None,
        };
        if let Some(name) = name.filter(|name| params.iter().any(|p| p == name)) {
            return match bindings.get(name) {
                Some(bound) => *bound == actual,
                None => {
                    bindings.insert(name.to_string(), actual);
                    true
                }
            };
        }
        let kind = self.type_ctx.borrow().interner.kind(actual).clone();
        match (formal, kind) {
            (Type::Array(inner), TyKind::Array(actual)) => {
                self.bind_type(inner, actual, params, bindings)
            }
            (Type::Tuple(formals), TyKind::Tuple(actuals)) => {
                formals.len() == actuals.len()
                    && formals
                        .iter()
                        .zip(actuals)
                        .all(|(f, a)| self.bind_type(f, a, params, bindings))
            }
            (Type::Function(formals, ret), TyKind::Func(actuals, actual_ret)) => {
                formals.len() == actuals.len()
                    && formals
                        .iter()
                        .zip(actuals)
                        .all(|(f, a)| self.bind_type(f, a, params, bindings))
                    && self.bind_type(ret, actual_ret, params, bindings)
            }
            (
                Type::Struct(token, Some(formals)),
                TyKind::Struct(def, actuals)
                | TyKind::Union(def, actuals)
                | TyKind::Interface(def, actuals),
            ) => {
                let matches = self.type_ctx.borrow().defs.name(def) == token.text;
                matches
                    && formals.len() == actuals.len()
                    && formals
                        .iter()
                        .zip(actuals)
                        .all(|(f, a)| self.bind_type(f, a, params, bindings))
            }
            _ => {
                let lowered = self.type_ctx.borrow_mut().lower_with(formal, bindings);
                let ctx = self.type_ctx.borrow();
                lowered == actual || matches!(ctx.interner.kind(lowered), TyKind::Error)
            }
        }
    }
}
