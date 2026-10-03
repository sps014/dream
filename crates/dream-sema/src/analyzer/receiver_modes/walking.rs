use super::*;

pub(super) fn is_self_expr(expr: &ExpressionNode, aliases: &[String]) -> bool {
    match expr {
        ExpressionNode::Identifier(t) => t.text == "this" || aliases.contains(&t.text),
        _ => false,
    }
}

/// Classify the receiver of a method call: bare `this`/alias, or `this.<field>` / `<alias>.<field>`.
pub(super) fn classify_receiver(expr: &ExpressionNode, aliases: &[String]) -> Option<RecvKind> {
    match expr {
        ExpressionNode::Identifier(t) => {
            if t.text == "this" || aliases.contains(&t.text) {
                Some(RecvKind::This)
            } else {
                None
            }
        }
        ExpressionNode::MemberAccess(base, member) => {
            let base_is_self = match &**base {
                ExpressionNode::Identifier(t) => t.text == "this" || aliases.contains(&t.text),
                _ => false,
            };
            base_is_self.then_some(RecvKind::Field(member.text.clone()))
        }
        ExpressionNode::Parenthesized(_, inner) => classify_receiver(inner, aliases),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn walk_body_for_facts(
    stmts: &[StatementNode],
    field_names: &[String],
    direct_unique: &mut bool,
    first_mutate_span: &mut Option<TextSpan>,
    raw_calls: &mut Vec<(RecvKind, String, TextSpan)>,
) {
    let mut aliases: Vec<String> = Vec::new();
    walk_statements(
        stmts,
        field_names,
        &mut aliases,
        direct_unique,
        first_mutate_span,
        raw_calls,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn walk_statements(
    stmts: &[StatementNode],
    field_names: &[String],
    aliases: &mut Vec<String>,
    direct_unique: &mut bool,
    first_mutate_span: &mut Option<TextSpan>,
    raw_calls: &mut Vec<(RecvKind, String, TextSpan)>,
) {
    for stmt in stmts {
        walk_statement(
            stmt,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn walk_statement(
    stmt: &StatementNode,
    field_names: &[String],
    aliases: &mut Vec<String>,
    direct_unique: &mut bool,
    first_mutate_span: &mut Option<TextSpan>,
    raw_calls: &mut Vec<(RecvKind, String, TextSpan)>,
) {
    match stmt {
        StatementNode::Assignment(name_tok, value) => {
            if field_names.contains(&name_tok.text) {
                mark_mutation(name_tok, direct_unique, first_mutate_span);
            }
            walk_expression(
                value,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::Declaration(name_tok, _, init, _) => {
            // `let alias = this;` tracks the local as a `this` alias for receiver classification.
            if is_self_expr(init, aliases) {
                aliases.push(name_tok.text.clone());
            }
            walk_expression(
                init,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::TupleDeclaration { init, .. } => walk_expression(
            init,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        ),
        StatementNode::MemberAssignment(target, name, value) => {
            if is_self_expr(target, aliases) {
                mark_mutation(name, direct_unique, first_mutate_span);
            } else {
                note_chain_write(target, direct_unique, first_mutate_span);
                walk_expression(
                    target,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
            walk_expression(
                value,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::IndexAssignment(target, index, value) => {
            note_chain_write(target, direct_unique, first_mutate_span);
            walk_expression(
                index,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_expression(
                value,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::FunctionInvocation(callee, _, args) => {
            raw_calls.push((RecvKind::This, callee.text.clone(), callee.position));
            note_sink_args(
                args,
                aliases,
                field_names,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::MethodInvocation(receiver, name, _, args) => {
            if let Some(kind) = classify_receiver(receiver, aliases) {
                raw_calls.push((kind, name.text.clone(), name.position));
            }
            walk_expression(
                receiver,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            for a in args {
                walk_expression(
                    a,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
        }
        StatementNode::Return(Some(e)) => walk_expression(
            e,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        ),
        StatementNode::IfElse(cond, then_b, elifs, else_b) => {
            walk_expression(
                cond,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_statements(
                then_b,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            for (c, b) in elifs {
                walk_expression(
                    c,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
                walk_statements(
                    b,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
            if let Some(b) = else_b {
                walk_statements(
                    b,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
        }
        StatementNode::While(cond, body) | StatementNode::DoWhile(body, cond) => {
            walk_expression(
                cond,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_statements(
                body,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::For(init, cond, step, body) => {
            if let Some(s) = init {
                walk_statement(
                    s,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
            if let Some(c) = cond {
                walk_expression(
                    c,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
            if let Some(s) = step {
                walk_statement(
                    s,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
            walk_statements(
                body,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::Labeled(_, inner) => walk_statement(
            inner,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        ),
        StatementNode::ForEach(_, iterable, _, _, body) => {
            walk_expression(
                iterable,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_statements(
                body,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::Switch(subject, arms, default_b) => {
            walk_expression(
                subject,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            for (_, body) in arms {
                walk_statements(
                    body,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
            if let Some(b) = default_b {
                walk_statements(
                    b,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
        }
        StatementNode::Lock(target, body) => {
            walk_expression(
                target,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_statements(
                body,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        StatementNode::Overflow(_, _, body) => walk_statements(
            body,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        ),
        StatementNode::ExpressionStatement(e) | StatementNode::AwaitStmt(e) => walk_expression(
            e,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        ),
        _ => {}
    }
}

// Helper wrappers kept tiny so each call site above stays readable.
#[allow(clippy::too_many_arguments)]
pub(super) fn note_sink_args(
    args: &[ExpressionNode],
    aliases: &mut Vec<String>,
    field_names: &[String],
    direct_unique: &mut bool,
    first_mutate_span: &mut Option<TextSpan>,
    raw_calls: &mut Vec<(RecvKind, String, TextSpan)>,
) {
    for a in args {
        walk_expression(
            a,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        );
    }
}

pub(super) fn mark_mutation(
    tok: &dream_syntax::token::syntax_token::SyntaxToken,
    direct_unique: &mut bool,
    first_mutate_span: &mut Option<TextSpan>,
) {
    *direct_unique = true;
    if first_mutate_span.is_none() {
        *first_mutate_span = Some(tok.position);
    }
}

/// Any indexed write whose chain roots in this/alias counts as mutating instance state:
/// `items[i] = v`, `this.slots[i] = v`, `this.slots[i].value = v`, ...
pub(super) fn note_chain_write(
    target: &ExpressionNode,
    direct_unique: &mut bool,
    first_mutate_span: &mut Option<TextSpan>,
) {
    fn root_span(mut e: &ExpressionNode) -> Option<TextSpan> {
        loop {
            match e {
                ExpressionNode::Identifier(t) => return Some(t.position),
                ExpressionNode::MemberAccess(base, m) => {
                    if matches!(&**base, ExpressionNode::Identifier(ref t) if t.text == "this") {
                        return Some(m.position);
                    }
                    e = base;
                }
                ExpressionNode::IndexAccess(base, _) => e = base,
                ExpressionNode::Parenthesized(_, inner) => e = inner,
                _ => return None,
            }
        }
    }
    if roots_in_self(target) && !*direct_unique {
        *direct_unique = true;
        if first_mutate_span.is_none() {
            *first_mutate_span = root_span(target);
        }
    }
}

/// True when the expression's chain roots at `this` (or an alias).
pub(super) fn roots_in_self(mut e: &ExpressionNode) -> bool {
    loop {
        match e {
            ExpressionNode::Identifier(t) => return t.text == "this",
            ExpressionNode::MemberAccess(base, _) => e = base,
            ExpressionNode::IndexAccess(base, _) => e = base,
            ExpressionNode::Parenthesized(_, inner) => e = inner,
            _ => return false,
        }
    }
}

#[allow(clippy::too_many_arguments)]
mod expressions;
use expressions::walk_expression;
