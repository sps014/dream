use super::*;

impl<'s> Extractor<'s> {
    pub(super) fn walk_block(
        &mut self,
        stmts: &[StatementNode],
        class_fields: &[String],
    ) {
        for s in stmts {
            self.walk_stmt(s, class_fields);
        }
    }

    pub(super) fn walk_args(
        &mut self,
        args: &[ExpressionNode],
        class_fields: &[String],
    ) {
        for a in args {
            self.walk_expr(a, class_fields);
        }
    }

    pub(super) fn emit_call_events(
        &mut self,
        receiver: &ExpressionNode,
        name: &str,
        args: &[ExpressionNode],
        class_fields: &[String],
    ) {
        match canonical_chain_from(receiver) {
            Some(recv_key) => {
                // Calling through a cursor keeps it alive — record the reference.
                self.events.push(Ev::Ref {
                    name: recv_key.clone(),
                });
                if let Some(span) = init_span(receiver) {
                    self.events.push(Ev::UniqueCandidate {
                        recv: recv_key,
                        name: name.to_string(),
                        span,
                    });
                }
            }
            // Chained receivers (`a.b(c).d(e)`): recurse so every nested call is seen.
            None => self.walk_expr(receiver, class_fields),
        }
        self.walk_args(args, class_fields);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn walk_stmt(
        &mut self,
        stmt: &StatementNode,
        class_fields: &[String],
    ) {
        match stmt {
            StatementNode::Declaration(name_tok, _, init, _) => {
                if is_self_expr(init, &self.aliases_this) {
                    self.aliases_this.push(name_tok.text.clone());
                }
                // Constructor-call class inference: `let xs = List<int>();`,
                // `let w = Widget(3);` — callee identifier names a declared class.
                #[allow(clippy::collapsible_match)]
                let ctor_class: Option<&String> = match init {
                    ExpressionNode::FunctionCall(callee, _, _) => {
                        self.class_names.get(&callee.text)
                    }
                    ExpressionNode::Call(callee_expr, _, _) => match &**callee_expr {
                        ExpressionNode::Identifier(t) => self.class_names.get(&t.text),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(class) = ctor_class {
                    self.local_class.retain(|(n, _)| n != &name_tok.text);
                    self.local_class
                        .push((name_tok.text.clone(), class.clone()));
                } else if std::env::var("DREAM_TRACE_BORROW").is_ok() {
                    eprintln!(
                        "[borrow] no ctor class for '{}' init={:?}",
                        name_tok.text, init,
                    );
                }
                match init {
                    ExpressionNode::ArrayLiteral(..) => {
                        self.local_class.retain(|(n, _)| n != &name_tok.text);
                        self.local_class
                            .push((name_tok.text.clone(), "__array".to_string()));
                    }
                    ExpressionNode::SetLiteral(..) => {
                        self.local_class.retain(|(n, _)| n != &name_tok.text);
                        self.local_class
                            .push((name_tok.text.clone(), "Set".to_string()));
                    }
                    ExpressionNode::MapLiteral(..) => {
                        self.local_class.retain(|(n, _)| n != &name_tok.text);
                        self.local_class
                            .push((name_tok.text.clone(), "Map".to_string()));
                    }
                    _ => {}
                }
                self.emit_binding_and_init(&name_tok.text, init, class_fields);
            }
            StatementNode::TupleDeclaration { init, .. } => {
                self.walk_expr(init, class_fields);
            }
            StatementNode::Assignment(name_tok, value) => {
                if class_fields.contains(&name_tok.text) {
                    if let Some(span) = Some(name_tok.position) {
                        self.events.push(Ev::UniqueCandidate {
                            recv: "this".to_string(),
                            name: format!("set {}", name_tok.text),
                            span,
                        });
                    }
                }
                self.emit_binding_and_init(&name_tok.text, value, class_fields);
            }
            StatementNode::MemberAssignment(target, name, value) => {
                if let Some(key) = canonical_chain_from(target) {
                    if let Some(span) = Some(name.position) {
                        self.events.push(Ev::UniqueCandidate {
                            recv: key.clone(),
                            name: format!("set {}", name.text),
                            span,
                        });
                        if key.starts_with("this.") {
                            self.events.push(Ev::UniqueCandidate {
                                recv: "this".to_string(),
                                name: format!("set {}", name.text),
                                span,
                            });
                        }
                    }
                } else {
                    self.walk_expr(target, class_fields);
                }
                self.walk_expr(value, class_fields);
            }
            StatementNode::IndexAssignment(target, index, value) => {
                if let Some(key) = canonical_chain_from(target) {
                    if let Some(span) = target_span_opt(target) {
                        self.events.push(Ev::UniqueCandidate {
                            recv: key.clone(),
                            name: "set_index".to_string(),
                            span,
                        });
                        if key.starts_with("this.") {
                            self.events.push(Ev::UniqueCandidate {
                                recv: "this".to_string(),
                                name: "set_index".to_string(),
                                span,
                            });
                        }
                    }
                } else {
                    self.walk_expr(target, class_fields);
                }
                self.walk_expr(index, class_fields);
                self.walk_expr(value, class_fields);
            }
            StatementNode::FunctionInvocation(callee, _, args) => {
                self.walk_args(args, class_fields);
                let _ = callee;
            }
            StatementNode::MethodInvocation(receiver, name, _, args) => {
                self.emit_call_events(receiver, &name.text, args, class_fields);
            }
            StatementNode::AwaitStmt(e) => self.walk_expr(e, class_fields),
            StatementNode::Return(Some(e)) => self.walk_expr(e, class_fields),
            StatementNode::IfElse(cond, then_b, elifs, else_b) => {
                self.walk_expr(cond, class_fields);
                self.walk_block(then_b, class_fields);
                for (c, b) in elifs {
                    self.walk_expr(c, class_fields);
                    self.walk_block(b, class_fields);
                }
                if let Some(b) = else_b {
                    self.walk_block(b, class_fields);
                }
            }
            StatementNode::While(cond, body) | StatementNode::DoWhile(body, cond) => {
                self.walk_expr(cond, class_fields);
                self.walk_block(body, class_fields);
            }
            StatementNode::For(init, cond, step, body) => {
                if let Some(s) = init {
                    self.walk_stmt(s, class_fields);
                }
                if let Some(c) = cond {
                    self.walk_expr(c, class_fields);
                }
                if let Some(s) = step {
                    self.walk_stmt(s, class_fields);
                }
                self.walk_block(body, class_fields);
            }
            StatementNode::Labeled(_, inner) => self.walk_stmt(inner, class_fields),
            StatementNode::ForEach(_, iterable, _, _, body) => {
                if let (Some(u), Some(span)) = (canonical_chain_from(iterable), init_span(iterable))
                {
                    self.events.push(Ev::ScopedOpen {
                        underlying: u,
                        span,
                    });
                    self.walk_block(body, class_fields);
                    self.events.push(Ev::ScopedClose);
                } else {
                    self.walk_expr(iterable, class_fields);
                    self.walk_block(body, class_fields);
                }
            }
            StatementNode::Switch(subject, arms, default_b) => {
                self.walk_expr(subject, class_fields);
                for (_, body) in arms {
                    self.walk_block(body, class_fields);
                }
                if let Some(b) = default_b {
                    self.walk_block(b, class_fields);
                }
            }
            StatementNode::Lock(target, body) => {
                self.walk_expr(target, class_fields);
                self.walk_block(body, class_fields);
            }
            StatementNode::Overflow(_, _, body) => self.walk_block(body, class_fields),
            StatementNode::ExpressionStatement(e) => self.walk_expr(e, class_fields),
            _ => {}
        }
    }

    pub(super) fn walk_expr(
        &mut self,
        e: &ExpressionNode,
        class_fields: &[String],
    ) {
        match e {
            ExpressionNode::Identifier(t) => {
                self.events.push(Ev::Ref {
                    name: t.text.clone(),
                });
            }
            ExpressionNode::Binary(lhs, _, rhs) => {
                self.walk_expr(lhs, class_fields);
                self.walk_expr(rhs, class_fields);
            }
            ExpressionNode::Ternary(cond, then_e, else_e) => {
                self.walk_expr(cond, class_fields);
                self.walk_expr(then_e, class_fields);
                self.walk_expr(else_e, class_fields);
            }
            ExpressionNode::Unary(_, inner)
            | ExpressionNode::Parenthesized(_, inner)
            | ExpressionNode::Try(inner) => self.walk_expr(inner, class_fields),
            ExpressionNode::IncDec { target, .. } => {
                if let Some(key) = canonical_chain_from(target) {
                    if let Some(span) = target_span_opt(target) {
                        self.events.push(Ev::UniqueCandidate {
                            recv: key,
                            name: "incdec".to_string(),
                            span,
                        });
                    }
                }
                self.walk_expr(target, class_fields);
            }
            ExpressionNode::ArrayLiteral(_, elems)
            | ExpressionNode::TupleLiteral(_, elems)
            | ExpressionNode::SetLiteral(_, elems) => {
                self.walk_args(elems, class_fields);
            }
            ExpressionNode::MapLiteral(_, pairs) => {
                for (k, v) in pairs {
                    self.walk_expr(k, class_fields);
                    self.walk_expr(v, class_fields);
                }
            }
            ExpressionNode::FunctionCall(callee, _, args) => {
                self.walk_args(args, class_fields);
                let _ = callee;
            }
            ExpressionNode::Call(callee, _, args) => {
                self.walk_expr(callee, class_fields);
                self.walk_args(args, class_fields);
            }
            ExpressionNode::MethodCall(receiver, name, _, args) => {
                self.emit_call_events(receiver, &name.text, args, class_fields);
            }
            ExpressionNode::IndexAccess(base, index) => {
                self.walk_expr(base, class_fields);
                self.walk_expr(index, class_fields);
            }
            ExpressionNode::Cast(_, _, inner)
            | ExpressionNode::IsExpression(inner, _, _)
            | ExpressionNode::Await(_, inner) => self.walk_expr(inner, class_fields),
            ExpressionNode::MemberAccess(base, _) => {
                self.walk_expr(base, class_fields)
            }
            ExpressionNode::Switch(_, subject, arms) => {
                self.walk_expr(subject, class_fields);
                for arm in arms {
                    match &arm.body {
                        dream_syntax::nodes::expression::SwitchArmBody::Expr(expr) => {
                            self.walk_expr(expr, class_fields)
                        }
                        dream_syntax::nodes::expression::SwitchArmBody::Block(stmts) => {
                            self.walk_block(stmts, class_fields)
                        }
                    }
                }
            }
            ExpressionNode::Lambda(_) => {
                // Lifted bodies analyzed separately.
            }
            ExpressionNode::NamedArg(_, inner) | ExpressionNode::RefArgument(_, inner) => {
                self.walk_expr(inner, class_fields)
            }
            _ => {}
        }
    }
}

fn target_span_opt(t: &ExpressionNode) -> Option<TextSpan> {
    pub(super) fn deepest(e: &ExpressionNode) -> Option<TextSpan> {
        match e {
            ExpressionNode::Identifier(t) => Some(t.position),
            ExpressionNode::MemberAccess(_, m) => Some(m.position),
            ExpressionNode::IndexAccess(b, _) => deepest(b),
            _ => None,
        }
    }
    deepest(t)
}
