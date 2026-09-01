use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{ExpressionNode, FunctionNode, StatementNode, SwitchArmBody, Type};

#[derive(Clone, Copy, Default)]
struct Flow {
    returns: bool,
    breaks: bool,
    continues: bool,
    falls_through: bool,
}

impl Flow {
    fn new_fallthrough() -> Self {
        Self {
            returns: false,
            breaks: false,
            continues: false,
            falls_through: true,
        }
    }

    fn merge_branch(self, other: Self) -> Self {
        Self {
            returns: self.returns || other.returns,
            breaks: self.breaks || other.breaks,
            continues: self.continues || other.continues,
            falls_through: self.falls_through || other.falls_through,
        }
    }
}

pub struct FunctionControlGraph<'a, 'd> {
    function: &'a FunctionNode<'a>,
    diagnostics: &'d mut DiagnosticBag,
}

impl<'a, 'd> FunctionControlGraph<'a, 'd> {
    pub fn new(function: &'a FunctionNode<'a>, diagnostics: &'d mut DiagnosticBag) -> Self {
        Self {
            function,
            diagnostics,
        }
    }

    pub fn build(&mut self) {
        if self.function.is_extern {
            return;
        }

        let flow = self.visit_block(self.function.body);

        if flow.falls_through {
            if let Some(ret) = &self.function.return_type {
                if ret != &Type::Void {
                    self.diagnostics.report_error(
                        format!(
                            "function '{}': not all code paths return a value",
                            self.function.name.text
                        ),
                        Some(self.function.name.position),
                    );
                }
            }
        }
    }

    fn visit_block(&mut self, stmts: &[StatementNode<'a>]) -> Flow {
        let mut current = Flow::new_fallthrough();
        let mut unreachable_reported = false;

        for stmt in stmts {
            if !current.falls_through {
                if !unreachable_reported {
                    // Only warn on actual statements that emit code or affect flow
                    self.diagnostics
                        .report_warning("unreachable code".to_string(), stmt_position(stmt));
                    unreachable_reported = true;
                }
                // Even if unreachable, we can keep walking to validate children or just skip.
                // We just skip execution flow since it's dead.
            } else {
                let stmt_flow = self.visit_stmt(stmt);
                current.returns |= stmt_flow.returns;
                current.breaks |= stmt_flow.breaks;
                current.continues |= stmt_flow.continues;
                current.falls_through = stmt_flow.falls_through;
            }
        }
        current
    }

    fn visit_stmt(&mut self, stmt: &StatementNode<'a>) -> Flow {
        match stmt {
            StatementNode::Return(_) => Flow {
                returns: true,
                breaks: false,
                continues: false,
                falls_through: false,
            },
            StatementNode::Break(_) => Flow {
                returns: false,
                breaks: true,
                continues: false,
                falls_through: false,
            },
            StatementNode::Continue(_) => Flow {
                returns: false,
                breaks: false,
                continues: true,
                falls_through: false,
            },
            StatementNode::IfElse(_, if_body, else_ifs, else_body) => {
                let mut f = self.visit_block(if_body);
                for (_, elif_body) in else_ifs {
                    f = f.merge_branch(self.visit_block(elif_body));
                }
                if let Some(eb) = else_body {
                    f = f.merge_branch(self.visit_block(eb));
                } else {
                    f.falls_through = true;
                }
                f
            }
            StatementNode::Switch(_, cases, default_body) => {
                let mut f = Flow::default();
                for (_, body) in cases {
                    f = f.merge_branch(self.visit_block(body));
                }
                if let Some(db) = default_body {
                    f = f.merge_branch(self.visit_block(db));
                } else {
                    f.falls_through = true;
                }
                f
            }
            StatementNode::ExpressionStatement(ExpressionNode::Switch(_, _, arms)) => {
                if arms.is_empty() {
                    return Flow::new_fallthrough();
                }
                let mut f = Flow::default();
                for arm in arms {
                    let mut arm_flow = match &arm.body {
                        SwitchArmBody::Block(body) => self.visit_block(body),
                        SwitchArmBody::Expr(_) => Flow::new_fallthrough(), // expression branches fall through
                    };
                    if arm.guard.is_some() {
                        // A failing guard continues to later arms; conservatively the switch
                        // may also fall out of the statement (exhaustiveness is a separate check).
                        arm_flow.falls_through = true;
                    }
                    f = f.merge_branch(arm_flow);
                }
                f
            }
            StatementNode::While(cond, body) => {
                let body_flow = self.visit_block(body);
                let is_true = is_literal_true(cond);
                Flow {
                    returns: body_flow.returns,
                    falls_through: !is_true || body_flow.breaks,
                    ..Flow::default()
                }
            }
            StatementNode::DoWhile(body, cond) => {
                let body_flow = self.visit_block(body);
                let is_true = is_literal_true(cond);
                Flow {
                    returns: body_flow.returns,
                    falls_through: body_flow.breaks
                        || ((body_flow.falls_through || body_flow.continues) && !is_true),
                    ..Flow::default()
                }
            }
            StatementNode::For(_, cond, _, body) => {
                let body_flow = self.visit_block(body);
                let is_true = cond.as_ref().map(|c| is_literal_true(c)).unwrap_or(true);
                Flow {
                    returns: body_flow.returns,
                    falls_through: !is_true || body_flow.breaks,
                    ..Flow::default()
                }
            }
            StatementNode::ForEach(_, _, _, _, body) => {
                let body_flow = self.visit_block(body);
                Flow {
                    returns: body_flow.returns,
                    falls_through: true,
                    ..Flow::default()
                }
            }
            StatementNode::Labeled(_, inner) => self.visit_stmt(inner),
            StatementNode::Lock(_, body) => self.visit_block(body),
            StatementNode::Defer(_, body) => {
                // Walk for nested unreachable warnings, but defer runs on scope exit so it
                // must not count as the enclosing function returning or stopping fallthrough.
                let _ = self.visit_block(body);
                Flow::new_fallthrough()
            }
            _ => Flow::new_fallthrough(),
        }
    }
}

fn is_literal_true(expr: &ExpressionNode) -> bool {
    matches!(expr, ExpressionNode::Literal(Type::Boolean(t)) if t.text == "true")
}

fn stmt_position(stmt: &StatementNode) -> Option<dream_text::text_span::TextSpan> {
    match stmt {
        StatementNode::Assignment(t, _) => Some(t.position),
        StatementNode::IndexAssignment(e, _, _) => e.position(),
        StatementNode::MemberAssignment(e, _, _) => e.position(),
        StatementNode::Declaration(t, _, _, _) => Some(t.position),
        StatementNode::TupleDeclaration { pattern, .. } => pattern.position(),
        StatementNode::FunctionInvocation(t, _, _) => Some(t.position),
        StatementNode::MethodInvocation(e, _, _, _) => e.position(),
        StatementNode::Return(Some(e)) => e.position(),
        StatementNode::Return(None) => None,
        StatementNode::IfElse(e, _, _, _) => e.position(),
        StatementNode::While(e, _) => e.position(),
        StatementNode::DoWhile(_, e) => e.position(),
        StatementNode::For(None, None, None, _) => None,
        StatementNode::For(Some(i), _, _, _) => stmt_position(i),
        StatementNode::For(None, Some(c), _, _) => c.position(),
        StatementNode::For(None, None, Some(i), _) => stmt_position(i),
        StatementNode::ExpressionStatement(e) => e.position(),
        StatementNode::AwaitStmt(e) => e.position(),
        StatementNode::ForEach(t, _, _, _, _) => Some(t.position),
        StatementNode::Switch(e, _, _) => e.position(),
        StatementNode::Lock(e, _) => e.position(),
        StatementNode::Defer(Some(budget), _) => budget.position(),
        StatementNode::Defer(None, _) => None,
        StatementNode::Break(_) | StatementNode::Continue(_) => None,
        StatementNode::Labeled(_, s) => stmt_position(s),
        StatementNode::WorkgroupDecl(t, _, _) => Some(t.position),
    }
}
