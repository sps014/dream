use super::*;
use dream::syntax::token::token_kind::TokenKind;

impl Builder {
    pub(crate) fn record_decl_type(&mut self, ty: &Type) {
        if let Some(index) = self.decls.len().checked_sub(1) {
            self.decl_types.insert(index, ty.clone());
        }
    }

    pub(crate) fn record_binding_type(
        &mut self,
        ty: Option<&Type>,
        expr: &ExpressionNode,
        scope: usize,
    ) {
        if let Some(ty) = ty {
            self.record_decl_type(ty);
        } else if let Some(id) = self.infer_id(expr, scope, &[])
            && let Some(index) = self.decls.len().checked_sub(1)
        {
            self.inferred_types.insert(index, id);
        }
    }

    pub(crate) fn record_callable(&mut self, func: &FunctionNode, owner: Option<&str>) {
        if let Some(index) = self.decls.len().checked_sub(1) {
            if let Some(owner) = owner {
                self.member_owners.insert(index, owner.to_string());
            }
            self.callables.insert(
                index,
                Callable {
                    params: func.parameters.clone(),
                    ret: func.return_type.clone().unwrap_or(Type::Void),
                    generics: param_names_from_tokens(&func.generic_parameters),
                    owner: owner.map(str::to_string),
                    is_async: func.is_async,
                },
            );
        }
    }

    pub(crate) fn display_id(&self, id: TypeId) -> String {
        let ctx = self.type_ctx.borrow();
        dream_types::display_name(&ctx.interner, &ctx.defs, id)
    }

    pub(crate) fn infer_type(&self, expr: &ExpressionNode, scope: usize) -> Option<String> {
        self.infer_id(expr, scope, &[])
            .map(|id| self.display_id(id))
    }

    fn decl_index(&self, decl: &Decl) -> Option<usize> {
        self.decls
            .iter()
            .position(|candidate| std::ptr::eq(candidate, decl))
    }

    fn decl_id(&self, decl: &Decl) -> Option<TypeId> {
        let index = self.decl_index(decl)?;
        if let Some(id) = self.inferred_types.get(&index) {
            return Some(*id);
        }
        if let Some(ty) = self.decl_types.get(&index) {
            return Some(self.type_ctx.borrow_mut().lower(ty));
        }
        let callable = self.callables.get(&index)?;
        let mut ctx = self.type_ctx.borrow_mut();
        let params = callable
            .params
            .iter()
            .map(|p| ctx.lower(&p.type_))
            .collect();
        let ret = ctx.lower(&callable.ret);
        let ret = call_result(&mut ctx, callable.is_async, ret)?;
        Some(ctx.interner.func(params, ret))
    }

    pub(crate) fn resolve_member_decl(&self, receiver: TypeId, name: &str) -> Option<&Decl> {
        self.decls
            .iter()
            .enumerate()
            .find(|(index, d)| {
                d.name == name
                    && matches!(d.kind, SymKind::Field | SymKind::Method)
                    && self
                        .member_owners
                        .get(index)
                        .is_some_and(|owner| self.owner_matches(owner, receiver))
            })
            .map(|(_, decl)| decl)
    }

    pub(crate) fn method_param_names(
        &self,
        recv: &ExpressionNode,
        method: &str,
        scope: usize,
    ) -> Option<Vec<String>> {
        let receiver = self.receiver_id(recv, scope, &[])?;
        let decl = self.resolve_member_decl(receiver, method)?;
        self.callables.get(&self.decl_index(decl)?).map(|callable| {
            callable
                .params
                .iter()
                .map(|p| p.name.text.clone())
                .collect()
        })
    }

    pub(crate) fn receiver_id(
        &self,
        recv: &ExpressionNode,
        scope: usize,
        extras: &[(String, TypeId)],
    ) -> Option<TypeId> {
        if let ExpressionNode::Identifier(token) = recv
            && let Some(decl) = self
                .resolve(&token.text, scope, token.position.start)
                .or_else(|| {
                    self.decls
                        .iter()
                        .find(|d| d.kind == SymKind::Type && d.name == token.text)
                })
            && matches!(
                decl.kind,
                SymKind::Class
                    | SymKind::Struct
                    | SymKind::Interface
                    | SymKind::Enum
                    | SymKind::Type
            )
        {
            return Some(
                self.type_ctx
                    .borrow_mut()
                    .lower(&Type::Struct(token.clone(), None)),
            );
        }
        self.infer_id(recv, scope, extras)
    }

    pub(crate) fn infer_id(
        &self,
        expr: &ExpressionNode,
        scope: usize,
        extras: &[(String, TypeId)],
    ) -> Option<TypeId> {
        match expr {
            ExpressionNode::Literal(ty) | ExpressionNode::Cast(_, ty, _) => {
                Some(self.type_ctx.borrow_mut().lower(ty))
            }
            ExpressionNode::SizeOf(..) => Some(self.type_ctx.borrow().interner.int()),
            ExpressionNode::NameOf(..) | ExpressionNode::TypeOf(..) => {
                Some(self.type_ctx.borrow().interner.string())
            }
            ExpressionNode::IsExpression(..) => Some(self.type_ctx.borrow().interner.bool()),
            ExpressionNode::Binary(left, op, right) => match op.kind {
                TokenKind::EqualEqualToken
                | TokenKind::NotEqualToken
                | TokenKind::GreaterThanToken
                | TokenKind::GreaterThanEqualToken
                | TokenKind::SmallerThanToken
                | TokenKind::SmallerThanEqualToken
                | TokenKind::AmpersandAmpersandToken
                | TokenKind::PipePipeToken => Some(self.type_ctx.borrow().interner.bool()),
                TokenKind::PlusToken
                | TokenKind::MinusToken
                | TokenKind::StarToken
                | TokenKind::SlashToken => self
                    .infer_id(left, scope, extras)
                    .or_else(|| self.infer_id(right, scope, extras)),
                _ => None,
            },
            ExpressionNode::Identifier(token) => extras
                .iter()
                .find(|(n, _)| n == &token.text)
                .map(|(_, id)| *id)
                .or_else(|| {
                    self.resolve(&token.text, scope, token.position.start)
                        .and_then(|d| self.decl_id(d))
                }),
            ExpressionNode::MemberAccess(recv, member) => {
                let receiver = self.receiver_id(recv, scope, extras)?;
                let decl = self.resolve_member_decl(receiver, &member.text)?;
                let ty = self.decl_types.get(&self.decl_index(decl)?)?;
                let bindings = self.receiver_bindings(receiver);
                Some(self.type_ctx.borrow_mut().lower_with(ty, &bindings))
            }
            ExpressionNode::FunctionCall(name, generics, args) => {
                let decl = self.resolve(&name.text, scope, name.position.start)?;
                if matches!(decl.kind, SymKind::Class | SymKind::Struct) {
                    return Some(
                        self.type_ctx
                            .borrow_mut()
                            .lower(&Type::Struct(name.clone(), generics.clone())),
                    );
                }
                if matches!(decl.kind, SymKind::Variable | SymKind::Param) {
                    let id = self.decl_id(decl)?;
                    return match self.type_ctx.borrow().interner.kind(id) {
                        TyKind::Func(_, ret) => Some(*ret),
                        _ => None,
                    };
                }
                let callable = self.callables.get(&self.decl_index(decl)?)?;
                self.infer_call(
                    callable,
                    None,
                    generics.as_deref().unwrap_or(&[]),
                    args,
                    scope,
                    extras,
                )
            }
            ExpressionNode::MethodCall(recv, name, generics, args) => {
                let receiver = self.receiver_id(recv, scope, extras)?;
                self.decls
                    .iter()
                    .enumerate()
                    .filter(|(index, d)| {
                        d.kind == SymKind::Method
                            && d.name == name.text
                            && self
                                .member_owners
                                .get(index)
                                .is_some_and(|owner| self.owner_matches(owner, receiver))
                    })
                    .find_map(|(index, _)| {
                        self.infer_call(
                            self.callables.get(&index)?,
                            Some(receiver),
                            generics.as_deref().unwrap_or(&[]),
                            args,
                            scope,
                            extras,
                        )
                    })
            }
            ExpressionNode::Call(callee, _, _) => {
                let id = self.infer_id(callee, scope, extras)?;
                match self.type_ctx.borrow().interner.kind(id) {
                    TyKind::Func(_, ret) => Some(*ret),
                    _ => None,
                }
            }
            ExpressionNode::Parenthesized(_, inner)
            | ExpressionNode::NamedArg(_, inner)
            | ExpressionNode::RefArgument(_, inner) => self.infer_id(inner, scope, extras),
            ExpressionNode::Await(_, inner) => {
                let id = self.infer_id(inner, scope, extras)?;
                let ctx = self.type_ctx.borrow();
                match ctx.interner.kind(id) {
                    TyKind::Struct(def, args)
                        if ctx.defs.name(*def) == dream::syntax::nodes::types::FUTURE_TYPE =>
                    {
                        args.first().copied()
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

pub(super) fn param_names_from_tokens(params: &Option<Vec<SyntaxToken>>) -> Vec<String> {
    params
        .as_ref()
        .map(|p| p.iter().map(|p| p.text.clone()).collect())
        .unwrap_or_default()
}

pub(super) fn call_result(ctx: &mut TypeCtx, is_async: bool, ret: TypeId) -> Option<TypeId> {
    if !is_async {
        return Some(ret);
    }
    let future = ctx.resolve(DefKind::Struct, dream::syntax::nodes::types::FUTURE_TYPE)?;
    if matches!(ctx.interner.kind(ret), TyKind::Struct(def, _) if *def == future) {
        return Some(ret);
    }
    Some(ctx.interner.struct_ty(future, vec![ret]))
}
