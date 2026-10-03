use super::*;

pub(super) fn walk_expression(
    e: &ExpressionNode,
    field_names: &[String],
    aliases: &mut Vec<String>,
    direct_unique: &mut bool,
    first_mutate_span: &mut Option<TextSpan>,
    raw_calls: &mut Vec<(RecvKind, String, TextSpan)>,
) {
    match e {
        ExpressionNode::Binary(lhs, _, rhs) => {
            walk_expression(
                lhs,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_expression(
                rhs,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        ExpressionNode::Ternary(cond, then_e, else_e) => {
            walk_expression(
                cond,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_expression(
                then_e,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_expression(
                else_e,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        ExpressionNode::Unary(_, inner)
        | ExpressionNode::Parenthesized(_, inner)
        | ExpressionNode::Try(inner) => walk_expression(
            inner,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        ),
        ExpressionNode::IncDec { target, .. } => {
            note_chain_write(target, direct_unique, first_mutate_span);
            if let ExpressionNode::Identifier(t) = &**target {
                if field_names.contains(&t.text) {
                    *direct_unique = true;
                    if first_mutate_span.is_none() {
                        *first_mutate_span = Some(t.position);
                    }
                }
            }
            walk_expression(
                target,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        ExpressionNode::ArrayLiteral(_, elems)
        | ExpressionNode::TupleLiteral(_, elems)
        | ExpressionNode::SetLiteral(_, elems) => {
            for x in elems {
                walk_expression(
                    x,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
        }
        ExpressionNode::MapLiteral(_, pairs) => {
            for (k, v) in pairs {
                walk_expression(
                    k,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
                walk_expression(
                    v,
                    field_names,
                    aliases,
                    direct_unique,
                    first_mutate_span,
                    raw_calls,
                );
            }
        }
        ExpressionNode::FunctionCall(callee, _, args) => {
            raw_calls.push((RecvKind::This, callee.text.clone(), callee.position));
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
        ExpressionNode::Call(callee, _, args) => {
            walk_expression(
                callee,
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
        ExpressionNode::MethodCall(receiver, name, _, args) => {
            if let Some(kind) = classify_receiver(receiver, aliases) {
                raw_calls.push((kind, name.text.clone(), name.position));
            }
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
        ExpressionNode::IndexAccess(base, index) => {
            walk_expression(
                base,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            walk_expression(
                index,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
        }
        ExpressionNode::Cast(_, _, inner)
        | ExpressionNode::IsExpression(inner, _, _)
        | ExpressionNode::Await(_, inner) => walk_expression(
            inner,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        ),
        ExpressionNode::MemberAccess(base, _) => walk_expression(
            base,
            field_names,
            aliases,
            direct_unique,
            first_mutate_span,
            raw_calls,
        ),
        ExpressionNode::Switch(_, subject, arms) => {
            walk_expression(
                subject,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            );
            for arm in arms {
                match &arm.body {
                    dream_syntax::nodes::expression::SwitchArmBody::Expr(expr) => walk_expression(
                        expr,
                        field_names,
                        aliases,
                        direct_unique,
                        first_mutate_span,
                        raw_calls,
                    ),
                    dream_syntax::nodes::expression::SwitchArmBody::Block(stmts) => {
                        walk_statements(
                            stmts,
                            field_names,
                            aliases,
                            direct_unique,
                            first_mutate_span,
                            raw_calls,
                        )
                    }
                }
            }
        }
        ExpressionNode::Lambda(l) => {
            // Lambda bodies are lifted and analyzed as their own functions elsewhere; captures
            // of `this` inside them belong to closure-cycle analysis, not receiver modes.
            let _ = l;
        }
        ExpressionNode::NamedArg(_, inner) | ExpressionNode::RefArgument(_, inner) => {
            walk_expression(
                inner,
                field_names,
                aliases,
                direct_unique,
                first_mutate_span,
                raw_calls,
            )
        }
        _ => {}
    }
}
