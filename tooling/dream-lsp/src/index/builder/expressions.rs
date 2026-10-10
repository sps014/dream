use super::*;

impl Builder {
    pub(crate) fn walk_expr(&mut self, expr: &ExpressionNode, scope: usize) {
        match expr {
            ExpressionNode::Identifier(token) => self.add_ref(token, SymKind::Variable, scope),
            ExpressionNode::Binary(l, _, r) => {
                self.walk_expr(l, scope);
                self.walk_expr(r, scope);
            }
            ExpressionNode::Unary(_, e)
            | ExpressionNode::IncDec { target: e, .. }
            | ExpressionNode::Parenthesized(_, e) => self.walk_expr(e, scope),
            ExpressionNode::FunctionCall(name, _, args) => {
                self.add_ref(name, SymKind::Function, scope);
                // A name resolves to a free function if one exists; otherwise `Name(...)` is a
                // constructor call, whose positional arguments are the custom `constructor`'s
                // parameters. A class with no explicit `constructor` has an implicit zero-arg
                // default constructor, so it contributes no positional parameter hints.
                let params = self
                    .fn_params
                    .get(&name.text)
                    .or_else(|| self.ctor_params.get(&name.text));
                if let Some(params) = params {
                    self.push_param_hints(&params.clone(), args);
                }
                for arg in args {
                    self.walk_expr(arg, scope);
                }
            }
            ExpressionNode::Call(callee, _, args) => {
                self.walk_expr(callee, scope);
                for arg in args {
                    self.walk_expr(arg, scope);
                }
            }
            ExpressionNode::IndexAccess(arr, idx) => {
                self.walk_expr(arr, scope);
                self.walk_expr(idx, scope);
            }
            ExpressionNode::Cast(_, ty, e) => {
                self.add_type_ref(ty, scope);
                self.walk_expr(e, scope);
            }
            ExpressionNode::MemberAccess(recv, member) => {
                self.walk_expr(recv, scope);
                // `Enum.Member` looks like member access on an identifier naming the enum.
                let kind = match recv {
                    ExpressionNode::Identifier(id) if self.is_enum(&id.text) => SymKind::EnumMember,
                    _ => SymKind::Field,
                };
                self.add_ref_with_receiver(member, kind, scope, receiver_ident(recv));
            }
            ExpressionNode::MethodCall(recv, method, _, args) => {
                self.walk_expr(recv, scope);
                // `Enum.Variant(...)` is a variant constructor, not a method call.
                let kind = match recv {
                    ExpressionNode::Identifier(id) if self.is_enum(&id.text) => SymKind::EnumMember,
                    _ => SymKind::Method,
                };
                self.add_ref_with_receiver(method, kind, scope, receiver_ident(recv));
                if let Some(params) = self.method_param_names(recv, &method.text, scope) {
                    self.push_param_hints(&params, args);
                }
                for arg in args {
                    self.walk_expr(arg, scope);
                }
            }
            ExpressionNode::IsExpression(e, ty, _) => {
                self.walk_expr(e, scope);
                self.add_type_ref(ty, scope);
            }
            ExpressionNode::Ternary(c, t, e) => {
                self.walk_expr(c, scope);
                self.walk_expr(t, scope);
                self.walk_expr(e, scope);
            }
            ExpressionNode::ArrayLiteral(_, elems)
            | ExpressionNode::SetLiteral(_, elems)
            | ExpressionNode::TupleLiteral(_, elems) => {
                for elem in elems {
                    self.walk_expr(elem, scope);
                }
            }
            ExpressionNode::MapLiteral(_, entries) => {
                for (k, v) in entries {
                    self.walk_expr(k, scope);
                    self.walk_expr(v, scope);
                }
            }
            ExpressionNode::ArrayRepeat(_, v, n) => {
                self.walk_expr(v, scope);
                self.walk_expr(n, scope);
            }
            ExpressionNode::Await(_, e) => self.walk_expr(e, scope),
            ExpressionNode::Switch(_, subject, arms) => {
                self.walk_expr(subject, scope);
                let subject_ty = self.infer_type(subject, scope);
                for arm in arms {
                    self.walk_pattern(&arm.pattern, scope, subject_ty.clone());
                    if let Some(guard) = &arm.guard {
                        self.walk_expr(guard, scope);
                    }
                    match &arm.body {
                        SwitchArmBody::Expr(e) => self.walk_expr(e, scope),
                        SwitchArmBody::Block(stmts) => self.walk_block(stmts, scope),
                    }
                }
            }
            ExpressionNode::Literal(_) => {}
            ExpressionNode::SizeOf(_, ty) => {
                self.add_type_ref(ty, scope);
            }
            ExpressionNode::NameOf(_, _) | ExpressionNode::DeclOf(_, _) => {}
            ExpressionNode::TypeOf(_, e) => self.walk_expr(e, scope),
            ExpressionNode::Try(e) => self.walk_expr(e, scope),
            ExpressionNode::Lambda(lambda) => {
                for param in &lambda.parameters {
                    let ty = param.type_.display_name();
                    let detail = format!("{}: {}", param.name.text, ty);
                    self.push_decl(&param.name, SymKind::Param, detail, scope, Some(ty));
                    self.record_decl_type(&param.type_);
                    self.add_type_ref(&param.type_, scope);
                }
                match &lambda.body {
                    LambdaBody::Expr(e) => self.walk_expr(e, scope),
                    LambdaBody::Block(stmts) => self.walk_block(stmts, scope),
                }
            }
            ExpressionNode::NamedArg(_, e) => self.walk_expr(e, scope),
            ExpressionNode::RefArgument(_, e) => self.walk_expr(e, scope),
            ExpressionNode::SyntaxBlock(block) => {
                for part in &block.parts {
                    if let SyntaxBlockPart::Splice(e) = part {
                        self.walk_expr(e, scope);
                    }
                }
            }
        }
    }

    /// Indexes the bindings and variant references introduced by a match pattern so hover, rename,
    /// and go-to work for them. Binding identifiers become local variables (typed from `expected`
    /// when the subject type is known — required for `Err(e) => { e.| }` member completion);
    /// variant names (and an optional `Enum.` qualifier) become references.
    pub(crate) fn walk_pattern(
        &mut self,
        pattern: &PatternNode,
        scope: usize,
        expected: Option<String>,
    ) {
        match pattern {
            PatternNode::Wildcard(_) | PatternNode::Literal(_) | PatternNode::Range(..) => {}
            PatternNode::Binding(name) => {
                let detail = match &expected {
                    Some(ty) => format!("{}: {}", name.text, ty),
                    None => "binding".to_string(),
                };
                self.push_decl(name, SymKind::Variable, detail, scope, expected);
            }
            PatternNode::Variant(qualifier, variant, subs) => {
                if let Some(q) = qualifier {
                    self.add_ref(q, self.type_name_kind(&q.text), scope);
                }
                self.add_ref(variant, SymKind::EnumMember, scope);
                let enum_name = qualifier
                    .as_ref()
                    .map(|q| q.text.as_str())
                    .or_else(|| expected.as_deref().map(type_base));
                let subject_args = expected
                    .as_deref()
                    .map(parse_angle_type_args)
                    .unwrap_or_default();
                let enum_params = enum_name
                    .and_then(|n| self.enum_type_params(n))
                    .unwrap_or_default();
                let payload_tys = self.variant_payload_types(enum_name, &variant.text);
                for (i, sub) in subs.iter().enumerate() {
                    let field_ty = payload_tys.get(i).cloned();
                    let concrete = field_ty.map(|t| {
                        if enum_params.is_empty() || subject_args.is_empty() {
                            t
                        } else {
                            substitute_named_type_params(&t, &enum_params, &subject_args)
                        }
                    });
                    self.walk_pattern(sub, scope, concrete);
                }
            }
            PatternNode::Or(alts) => {
                for alt in alts {
                    self.walk_pattern(alt, scope, expected.clone());
                }
            }
            PatternNode::Tuple(elems) => {
                for sub in elems {
                    self.walk_pattern(sub, scope, None);
                }
            }
        }
    }

    /// Generic parameter names declared on `enum Name<…>` (`Result` → `["T","E"]`).
    pub(crate) fn enum_type_params(&self, name: &str) -> Option<Vec<String>> {
        let detail = self
            .decls
            .iter()
            .find(|d| d.kind == SymKind::Enum && d.name == name)
            .map(|d| d.detail.as_str())?;
        let args = parse_angle_type_args(detail);
        if args.is_empty() { None } else { Some(args) }
    }

    /// Payload field types of `Enum.Variant` in declaration order (`Result.Err` → `["E"]`).
    pub(crate) fn variant_payload_types(
        &self,
        enum_name: Option<&str>,
        variant: &str,
    ) -> Vec<String> {
        let Some(en) = enum_name else {
            return Vec::new();
        };
        let prefix = format!("{en}.{variant}::");
        self.decls
            .iter()
            .filter(|d| d.kind == SymKind::Param && d.detail.starts_with(&prefix))
            .filter_map(|d| d.ty.clone())
            .collect()
    }

    pub(crate) fn add_type_ref(&mut self, ty: &Type, scope: usize) {
        if let Type::Struct(token, _) = base_struct(ty) {
            self.add_ref(token, self.type_name_kind(&token.text), scope);
        }
    }

    /// Concrete kind for a named type reference, falling back to [`SymKind::Type`] when unknown
    /// (builtins, unresolved names, generics not yet declared in this pass).
    pub(crate) fn type_name_kind(&self, name: &str) -> SymKind {
        self.decls
            .iter()
            .find(|d| {
                d.scope == GLOBAL
                    && d.name == name
                    && matches!(
                        d.kind,
                        SymKind::Class | SymKind::Struct | SymKind::Interface | SymKind::Enum
                    )
            })
            .map(|d| d.kind)
            .unwrap_or(SymKind::Type)
    }

    pub(crate) fn is_enum(&self, name: &str) -> bool {
        self.decls
            .iter()
            .any(|d| d.kind == SymKind::Enum && d.name == name)
    }
}
