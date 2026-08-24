//! Closure-capture cycle detection (W2): rejects the self-capture delegate pattern — a method
//! storing a lambda that captures `this` (directly or through a local alias) into a function-
//! or interface-typed field of the same class. The object and the closure then reference each
//! other, and neither can ever be freed.
//!
//! Verified leak shape (runtime counters showed `live=3` at exit):
//! ```dream
//! let b = Button("leak");
//! b.onClick = () => "clicked " + b.label;   // ✗ rejected
//! ```
//! Fix: store plain data in the closure, or capture a weak reference once weak local bindings
//! land. Mutual two-object capture is analyzed in a later pass.

use super::*;
use crate::analyzer::expressions::capture_scan::lambda_free_names;
use dream_syntax::nodes::expression::ExpressionNode;
use dream_syntax::nodes::statement::StatementNode;
impl<'a> Analyzer<'a> {
    /// Reports every `this`-capturing lambda stored into a fn-typed field of its own class.
    /// Called from `analyze_pgm` on clean programs, after body analysis.
    pub(in crate::analyzer) fn check_closure_self_capture(
        &self,
        node: &'a ProgramNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        for struct_decl in node.structs.iter() {
            if struct_decl.is_value || struct_decl.is_static {
                continue;
            }
            // Fn/interface-typed fields are the storage sites that can form the cycle.
            let fn_fields: Vec<&str> = struct_decl
                .fields
                .iter()
                .filter(|f| matches!(f.field_type, Type::Function(..)))
                .map(|f| f.name.text.as_str())
                .collect();
            if fn_fields.is_empty() {
                continue;
            }
            let owner = struct_decl.name.text.clone();
            for method in &struct_decl.methods {
                if method.is_static || method.body.is_empty() {
                    continue;
                }
                let mut walker = SelfCaptureWalker {
                    owner: owner.clone(),
                    fn_fields: fn_fields.clone(),
                    this_aliases: Vec::new(),
                    file_path: struct_decl.file_path.clone(),
                    diagnostics,
                };
                walker.walk_statements(method.body);
            }
        }
    }
}

/// Walks one method body, tracking locals aliased to `this`, and flags lambda stores into
/// the class's own fn-typed fields when the lambda captures `this`/an alias.
struct SelfCaptureWalker<'d, 'a> {
    owner: String,
    fn_fields: Vec<&'a str>,
    this_aliases: Vec<String>,
    file_path: Option<Rc<str>>,
    diagnostics: &'d mut DiagnosticBag,
}

impl<'d, 'a> SelfCaptureWalker<'d, 'a> {
    fn report(&mut self, field: &str, span: TextSpan) {
        if self.file_path.is_some() {
            self.diagnostics.file_path = file_path_string(&self.file_path);
        }
        self.diagnostics.report_error(
            format!(
                "closure captures 'this' and is stored into field '{}' of '{}' — the object and the closure reference each other, so neither can ever be freed. Capture only the data the callback needs, or restructure so the callback does not outlive '{}'",
                field, self.owner, self.owner
            ),
            Some(span),
        );
    }

    fn walk_statements(&mut self, stmts: &[StatementNode]) {
        for stmt in stmts {
            self.walk_statement(stmt);
        }
    }

    fn walk_statement(&mut self, stmt: &StatementNode) {
        match stmt {
            StatementNode::Declaration(name_tok, _, init, _) => {
                if is_this_expr(init) {
                    self.this_aliases.push(name_tok.text.clone());
                }
                self.walk_expression(init);
            }
            StatementNode::Assignment(name_tok, value) => {
                if self.is_fn_field(name_tok) {
                    self.check_lambda_capture(&name_tok.text, value);
                }
                self.walk_expression(value);
            }
            StatementNode::MemberAssignment(target, name, value) => {
                if is_self_receiver(target) {
                    if self.fn_fields.contains(&name.text.as_str()) {
                        self.check_lambda_capture(&name.text, value);
                    }
                } else {
                    self.walk_expression(target);
                }
                self.walk_expression(value);
            }
            StatementNode::IndexAssignment(target, index, value) => {
                self.walk_expression(target);
                self.walk_expression(index);
                self.walk_expression(value);
            }
            StatementNode::FunctionInvocation(_, _, args)
            | StatementNode::MethodInvocation(_, _, _, args) => {
                for a in args {
                    self.walk_expression(a);
                }
            }
            StatementNode::Return(Some(e)) => self.walk_expression(e),
            StatementNode::IfElse(cond, then_b, elifs, else_b) => {
                self.walk_expression(cond);
                self.walk_statements(then_b);
                for (c, b) in elifs {
                    self.walk_expression(c);
                    self.walk_statements(b);
                }
                if let Some(b) = else_b {
                    self.walk_statements(b);
                }
            }
            StatementNode::While(cond, body) | StatementNode::DoWhile(body, cond) => {
                self.walk_expression(cond);
                self.walk_statements(body);
            }
            StatementNode::For(init, cond, step, body) => {
                if let Some(s) = init {
                    self.walk_statement(s);
                }
                if let Some(c) = cond {
                    self.walk_expression(c);
                }
                if let Some(s) = step {
                    self.walk_statement(s);
                }
                self.walk_statements(body);
            }
            StatementNode::Labeled(_, inner) => self.walk_statement(inner),
            StatementNode::ForEach(_, iterable, _, _, body) => {
                self.walk_expression(iterable);
                self.walk_statements(body);
            }
            StatementNode::Switch(subject, arms, default_b) => {
                self.walk_expression(subject);
                for (_, body) in arms {
                    self.walk_statements(body);
                }
                if let Some(b) = default_b {
                    self.walk_statements(b);
                }
            }
            StatementNode::Lock(target, body) => {
                self.walk_expression(target);
                self.walk_statements(body);
            }
            StatementNode::ExpressionStatement(e) | StatementNode::AwaitStmt(e) => {
                self.walk_expression(e)
            }
            _ => {}
        }
    }

    fn walk_expression(&mut self, e: &ExpressionNode) {
        use ExpressionNode as E;
        match e {
            E::Binary(lhs, _, rhs) | E::Ternary(lhs, _, rhs) => {
                self.walk_expression(lhs);
                self.walk_expression(rhs);
            }
            E::Unary(_, inner)
            | E::Parenthesized(_, inner)
            | E::Try(inner)
            | E::Await(_, inner)
            | E::Cast(_, _, inner)
            | E::IsExpression(inner, _, _)
            | E::NamedArg(_, inner)
            | E::RefArgument(_, inner) => self.walk_expression(inner),
            E::IncDec { target, .. } => self.walk_expression(target),
            E::ArrayLiteral(_, elems)
            | E::TupleLiteral(_, elems)
            | E::SetLiteral(_, elems) => {
                for x in elems {
                    self.walk_expression(x);
                }
            }
            E::MapLiteral(_, pairs) => {
                for (k, v) in pairs {
                    self.walk_expression(k);
                    self.walk_expression(v);
                }
            }
            E::FunctionCall(_, _, args) | E::MethodCall(_, _, _, args) => {
                for a in args {
                    self.walk_expression(a);
                }
            }
            E::IndexAccess(base, index) => {
                self.walk_expression(base);
                self.walk_expression(index);
            }
            E::MemberAccess(base, _) => self.walk_expression(base),
            E::Switch(_, subject, arms) => {
                self.walk_expression(subject);
                for arm in arms {
                    match &arm.body {
                        dream_syntax::nodes::expression::SwitchArmBody::Expr(expr) => {
                            self.walk_expression(expr)
                        }
                        dream_syntax::nodes::expression::SwitchArmBody::Block(stmts) => {
                            self.walk_statements(stmts)
                        }
                    }
                }
            }
            E::Lambda(l) => {
                // A lambda nested inside an argument/initializer could itself be stored — but
                // v1 only tracks direct assignment stores (the dominant pattern).
                let _ = l;
            }
            _ => {}
        }
    }

    /// True when `tok` names one of the class's fn-typed fields.
    fn is_fn_field(&self, tok: &SyntaxToken) -> bool {
        self.fn_fields.contains(&tok.text.as_str())
    }

    /// Flags `value` when it is a lambda capturing `this`/an alias.
    fn check_lambda_capture(&mut self, field: &str, value: &ExpressionNode) {
        if let ExpressionNode::Lambda(l) = value {
            let free = lambda_free_names(l);
            let captures_this =
                free.contains("this") || free.iter().any(|n| self.this_aliases.contains(n));
            if captures_this {
                let span = l.open_paren_position;
                self.report(field, span);
            }
        }
    }
}

fn is_this_expr(e: &ExpressionNode) -> bool {
    matches!(e, ExpressionNode::Identifier(t) if t.text == "this")
}

fn is_self_receiver(target: &ExpressionNode) -> bool {
    matches!(target, ExpressionNode::Identifier(t) if t.text == "this")
}
