//! One exhaustive AST walk shared by every generator query (syntax sites, call sites, the
//! unexpanded-block check), so a new expression form is handled in exactly one place.

use crate::driver::source_loader::ProgramAccumulator;
use dream_syntax::nodes::{
    ExpressionNode, FunctionNode, LambdaBody, StatementNode, SwitchArmBody, SyntaxBlockPart,
};

pub(super) trait Visitor<'a> {
    /// Called before an expression's children are walked; return `false` to skip them.
    fn expr(&mut self, _e: &ExpressionNode<'a>) -> bool {
        true
    }
    /// Called before a statement's children are walked (`MethodInvocation` statements are calls).
    fn stmt(&mut self, _s: &StatementNode<'a>) {}
    /// Called after a statement's children are walked (bind locals here).
    fn after_stmt(&mut self, _s: &StatementNode<'a>) {}
    fn enter_scope(&mut self) {}
    fn exit_scope(&mut self) {}
    /// Called before each global initializer and function body with its declaring file.
    fn enter_file(&mut self, _file: Option<&str>) {}
    /// Called before a function body (after [`Visitor::enter_file`]).
    fn enter_function(&mut self, _f: &FunctionNode<'a>) {}
}

pub(super) fn walk_body<'a, V: Visitor<'a>>(v: &mut V, stmts: &[StatementNode<'a>]) {
    v.enter_scope();
    for s in stmts {
        walk_stmt(v, s);
    }
    v.exit_scope();
}

pub(super) fn walk_expr<'a, V: Visitor<'a>>(v: &mut V, expr: &ExpressionNode<'a>) {
    if !v.expr(expr) {
        return;
    }
    match expr {
        ExpressionNode::SyntaxBlock(block) => {
            for part in &block.parts {
                if let SyntaxBlockPart::Splice(e) = part {
                    walk_expr(v, e);
                }
            }
        }
        ExpressionNode::Binary(l, _, r) | ExpressionNode::IndexAccess(l, r) => {
            walk_expr(v, l);
            walk_expr(v, r);
        }
        ExpressionNode::Ternary(c, t, e) => {
            walk_expr(v, c);
            walk_expr(v, t);
            walk_expr(v, e);
        }
        ExpressionNode::Unary(_, x)
        | ExpressionNode::IncDec { target: x, .. }
        | ExpressionNode::Parenthesized(_, x)
        | ExpressionNode::Await(_, x)
        | ExpressionNode::Try(x)
        | ExpressionNode::Cast(_, _, x)
        | ExpressionNode::IsExpression(x, _, _)
        | ExpressionNode::TypeOf(_, x)
        | ExpressionNode::MemberAccess(x, _)
        | ExpressionNode::RefArgument(_, x)
        | ExpressionNode::NamedArg(_, x) => walk_expr(v, x),
        ExpressionNode::Call(c, _, args) | ExpressionNode::MethodCall(c, _, _, args) => {
            walk_expr(v, c);
            for a in args {
                walk_expr(v, a);
            }
        }
        ExpressionNode::ArrayRepeat(_, x, n) => {
            walk_expr(v, x);
            walk_expr(v, n);
        }
        ExpressionNode::FunctionCall(_, _, args)
        | ExpressionNode::ArrayLiteral(_, args)
        | ExpressionNode::TupleLiteral(_, args)
        | ExpressionNode::SetLiteral(_, args) => {
            for a in args {
                walk_expr(v, a);
            }
        }
        ExpressionNode::MapLiteral(_, entries) => {
            for (k, x) in entries {
                walk_expr(v, k);
                walk_expr(v, x);
            }
        }
        ExpressionNode::Switch(_, subj, arms) => {
            walk_expr(v, subj);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    walk_expr(v, g);
                }
                match &arm.body {
                    SwitchArmBody::Expr(e) => walk_expr(v, e),
                    SwitchArmBody::Block(stmts) => walk_body(v, stmts),
                }
            }
        }
        ExpressionNode::Lambda(l) => match &l.body {
            LambdaBody::Expr(e) => walk_expr(v, e),
            LambdaBody::Block(stmts) => walk_body(v, stmts),
        },
        ExpressionNode::Literal(_)
        | ExpressionNode::Identifier(_)
        | ExpressionNode::SizeOf(_, _)
        | ExpressionNode::NameOf(_, _)
        | ExpressionNode::DeclOf(_, _) => {}
    }
}

pub(super) fn walk_stmt<'a, V: Visitor<'a>>(v: &mut V, stmt: &StatementNode<'a>) {
    v.stmt(stmt);
    match stmt {
        StatementNode::ExpressionStatement(e)
        | StatementNode::AwaitStmt(e)
        | StatementNode::Return(Some(e))
        | StatementNode::Assignment(_, e)
        | StatementNode::Declaration(_, _, e, _)
        | StatementNode::TupleDeclaration { init: e, .. } => walk_expr(v, e),
        StatementNode::IndexAssignment(a, i, x) => {
            walk_expr(v, a);
            walk_expr(v, i);
            walk_expr(v, x);
        }
        StatementNode::MemberAssignment(r, _, x) => {
            walk_expr(v, r);
            walk_expr(v, x);
        }
        StatementNode::FunctionInvocation(_, _, args) => {
            for a in args {
                walk_expr(v, a);
            }
        }
        StatementNode::MethodInvocation(r, _, _, args) => {
            walk_expr(v, r);
            for a in args {
                walk_expr(v, a);
            }
        }
        StatementNode::IfElse(cond, then_b, elifs, else_b) => {
            walk_expr(v, cond);
            walk_body(v, then_b);
            for (c, b) in elifs {
                walk_expr(v, c);
                walk_body(v, b);
            }
            if let Some(b) = else_b {
                walk_body(v, b);
            }
        }
        StatementNode::While(cond, body)
        | StatementNode::Lock(cond, body)
        | StatementNode::DoWhile(body, cond) => {
            walk_expr(v, cond);
            walk_body(v, body);
        }
        StatementNode::Overflow(_, _, body) => walk_body(v, body),
        StatementNode::Defer(budget, body) => {
            if let Some(q) = budget {
                walk_expr(v, q);
            }
            walk_body(v, body);
        }
        StatementNode::For(init, cond, inc, body) => {
            v.enter_scope();
            if let Some(s) = init {
                walk_stmt(v, s);
            }
            if let Some(e) = cond {
                walk_expr(v, e);
            }
            if let Some(s) = inc {
                walk_stmt(v, s);
            }
            walk_body(v, body);
            v.exit_scope();
        }
        StatementNode::ForEach(_, iter, _, _, body) => {
            walk_expr(v, iter);
            walk_body(v, body);
        }
        StatementNode::Switch(subj, cases, default) => {
            walk_expr(v, subj);
            for (labels, body) in cases {
                for l in labels {
                    walk_expr(v, l);
                }
                walk_body(v, body);
            }
            if let Some(d) = default {
                walk_body(v, d);
            }
        }
        StatementNode::Labeled(_, inner) => walk_stmt(v, inner),
        StatementNode::Return(None) | StatementNode::Break(_) | StatementNode::Continue(_) => {}
    }
    v.after_stmt(stmt);
}

fn walk_function<'a, V: Visitor<'a>>(v: &mut V, f: &FunctionNode<'a>, file: Option<&str>) {
    v.enter_file(file);
    v.enter_function(f);
    walk_body(v, f.body);
}

/// Walks every function body and global initializer whose declaring file passes `include`.
/// Compiler-synthesized `extend` blocks are skipped.
pub(super) fn walk_program<'a, V: Visitor<'a>>(
    v: &mut V,
    acc: &ProgramAccumulator<'a>,
    include: &dyn Fn(Option<&str>) -> bool,
) {
    for g in &acc.all_globals {
        let file = g.file_path.as_deref();
        if include(file) {
            v.enter_file(file);
            walk_expr(v, &g.initializer);
        }
    }
    for f in &acc.all_functions {
        let file = f.file_path.as_deref();
        if include(file) {
            walk_function(v, f, file);
        }
    }
    for s in &acc.all_structs {
        let file = s.file_path.as_deref();
        if include(file) {
            for m in &s.methods {
                walk_function(v, m, file);
            }
        }
    }
    for e in &acc.all_enums {
        let file = e.file_path.as_deref();
        if include(file) {
            for m in &e.methods {
                walk_function(v, m, file);
            }
        }
    }
    for e in &acc.all_extends {
        let file = e.file_path.as_deref();
        if !e.is_synthesized && include(file) {
            for m in &e.methods {
                walk_function(v, m, file);
            }
        }
    }
}

/// User source (not embedded stdlib, not compiler-synthesized).
pub(super) fn is_user_file(file: Option<&str>) -> bool {
    file.is_some_and(|f| !dream_stdlib::is_std_source(f))
}
