use super::*;

impl Builder {
    pub(crate) fn walk_stmt(&mut self, stmt: &StatementNode, scope: usize) {
        match stmt {
            StatementNode::Declaration(name, ty, expr, _is_const) => {
                let inferred = self.infer_type(expr, scope);
                let type_str = ty
                    .as_ref()
                    .map(|t| t.display_name())
                    .or_else(|| inferred.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                let detail = type_str.clone();
                let resolved_ty = ty.as_ref().map(|t| t.display_name()).or(inferred);
                self.push_decl(name, SymKind::Variable, detail, scope, resolved_ty.clone());
                self.record_binding_type(ty.as_ref(), expr, scope);
                if let Some(t) = ty {
                    self.add_type_ref(t, scope);
                } else if let Some(t_str) = resolved_ty {
                    self.inlay_hints.push(InlayHintOut {
                        offset: name.position.end,
                        label: format!(": {}", t_str),
                        kind: InlayKind::Type,
                    });
                }
                self.walk_expr(expr, scope);
            }
            StatementNode::TupleDeclaration {
                pattern, ty, init, ..
            } => {
                if let Some(t) = ty {
                    self.add_type_ref(t, scope);
                }
                self.walk_expr(init, scope);
                for name in pattern.binding_names() {
                    let type_str = ty
                        .as_ref()
                        .map(|t| t.display_name())
                        .unwrap_or_else(|| "unknown".to_string());
                    self.push_decl(
                        name,
                        SymKind::Variable,
                        type_str.clone(),
                        scope,
                        Some(type_str),
                    );
                }
            }
            StatementNode::Assignment(name, expr) => {
                self.add_ref(name, SymKind::Variable, scope);
                self.walk_expr(expr, scope);
            }
            StatementNode::IndexAssignment(target, index, value) => {
                self.walk_expr(target, scope);
                self.walk_expr(index, scope);
                self.walk_expr(value, scope);
            }
            StatementNode::MemberAssignment(target, member, value) => {
                self.walk_expr(target, scope);
                self.add_ref_with_receiver(member, SymKind::Field, scope, receiver_ident(target));
                self.walk_expr(value, scope);
            }
            StatementNode::Return(Some(expr)) => self.walk_expr(expr, scope),
            StatementNode::Return(None) => {}
            StatementNode::FunctionInvocation(name, _, args) => {
                self.add_ref(name, SymKind::Function, scope);
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
            StatementNode::ExpressionStatement(expr) => {
                self.walk_expr(expr, scope);
            }
            StatementNode::MethodInvocation(recv, method, _, args) => {
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
            StatementNode::IfElse(cond, then_body, else_ifs, else_body) => {
                self.walk_expr(cond, scope);
                self.walk_block(then_body, scope);
                for (c, body) in else_ifs {
                    self.walk_expr(c, scope);
                    self.walk_block(body, scope);
                }
                if let Some(body) = else_body {
                    self.walk_block(body, scope);
                }
            }
            StatementNode::While(cond, body) => {
                self.walk_expr(cond, scope);
                self.walk_block(body, scope);
            }
            StatementNode::DoWhile(body, cond) => {
                self.walk_block(body, scope);
                self.walk_expr(cond, scope);
            }
            StatementNode::For(init, cond, update, body) => {
                if let Some(s) = init {
                    self.walk_stmt(s, scope);
                }
                if let Some(c) = cond {
                    self.walk_expr(c, scope);
                }
                if let Some(s) = update {
                    self.walk_stmt(s, scope);
                }
                self.walk_block(body, scope);
            }
            StatementNode::ForEach(var, iterable, _, _, body) => {
                let detail = "unknown".to_string();
                self.push_decl(var, SymKind::Variable, detail, scope, None);
                self.walk_expr(iterable, scope);
                self.walk_block(body, scope);
            }
            StatementNode::Labeled(_, inner) => self.walk_stmt(inner, scope),
            StatementNode::AwaitStmt(expr) => self.walk_expr(expr, scope),
            StatementNode::Break(_) | StatementNode::Continue(_) => {}
            StatementNode::Switch(subject, cases, default) => {
                self.walk_expr(subject, scope);
                for (labels, body) in cases {
                    for label in labels {
                        self.walk_expr(label, scope);
                    }
                    self.walk_block(body, scope);
                }
                if let Some(body) = default {
                    self.walk_block(body, scope);
                }
            }
            StatementNode::Lock(target, body) => {
                self.walk_expr(target, scope);
                self.walk_block(body, scope);
            }
            StatementNode::Defer(budget, body) => {
                if let Some(q) = budget {
                    self.walk_expr(q, scope);
                }
                self.walk_block(body, scope);
            }
            StatementNode::Overflow(_, _, body) => self.walk_block(body, scope),
            StatementNode::WorkgroupDecl(_, _, _) => {}
        }
    }

    pub(crate) fn walk_block(&mut self, body: &[StatementNode], scope: usize) {
        for stmt in body {
            self.walk_stmt(stmt, scope);
        }
    }

    /// Emits a parameter-name inlay hint (`name:`) before each positional argument of a call. The
    /// hint is suppressed when the argument is simply the identifier matching the parameter name,
    /// which would be redundant. Extra arguments (more than parameters) are left unannotated.
    pub(crate) fn push_param_hints(&mut self, params: &[String], args: &[ExpressionNode]) {
        for (param, arg) in params.iter().zip(args.iter()) {
            // An explicit `name: value` argument already shows its parameter name in source.
            if matches!(arg, ExpressionNode::NamedArg(..)) {
                continue;
            }
            if let ExpressionNode::Identifier(tok) = arg {
                if &tok.text == param {
                    continue;
                }
            }
            if let Some(span) = arg.start_position() {
                self.inlay_hints.push(InlayHintOut {
                    offset: span.start,
                    label: format!("{}:", param),
                    kind: InlayKind::Parameter,
                });
            }
        }
    }
}
