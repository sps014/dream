use super::collection_stmts::collect_collections_from_stmts;
use super::collection_types::collect_collections_from_type;
use super::collection_types::CollectionSpec;
use dream_syntax::nodes::expression::ExpressionNode;
use dream_syntax::nodes::Type;
use std::collections::BTreeSet;
use std::collections::HashSet;

pub(super) fn is_json_codec_method(name: &str) -> bool {
    matches!(name, "serialize" | "deserialize" | "from_value")
}

/// Reconstructs a collection type from an AST expression when `Json.serialize` has no type args.
pub(super) fn infer_expr_type(
    expr: &ExpressionNode<'_>,
    locals: &[(String, Type)],
) -> Option<Type> {
    match expr {
        ExpressionNode::Identifier(tok) => locals
            .iter()
            .rev()
            .find(|(n, _)| n == &tok.text)
            .map(|(_, t)| t.clone()),
        ExpressionNode::Parenthesized(_, inner) => infer_expr_type(inner, locals),
        ExpressionNode::Cast(_, ty, _) => Some(ty.clone()),
        ExpressionNode::FunctionCall(name, type_args, _) => match name.text.as_str() {
            "List" | "Set" | "Map" | "SortedMap" => {
                Some(Type::Struct(name.clone(), type_args.clone()))
            }
            _ => None,
        },
        ExpressionNode::NamedArg(_, inner) | ExpressionNode::RefArgument(_, inner) => {
            infer_expr_type(inner, locals)
        }
        _ => None,
    }
}

pub(super) fn collect_json_codec_call(
    receiver: &ExpressionNode<'_>,
    method: &str,
    type_args: &Option<Vec<Type>>,
    args: &[ExpressionNode<'_>],
    jsonable: &HashSet<String>,
    locals: &[(String, Type)],
    out: &mut BTreeSet<CollectionSpec>,
) {
    if !is_json_static_receiver(receiver) || !is_json_codec_method(method) {
        return;
    }
    if let Some(types) = type_args {
        for ty in types {
            collect_collections_from_type(ty, jsonable, out);
        }
        return;
    }
    if method == "serialize" || method == "serialize_pretty" {
        if let Some(arg) = args.first() {
            if let Some(ty) = infer_expr_type(arg, locals) {
                collect_collections_from_type(&ty, jsonable, out);
            }
        }
    }
}

pub(super) fn collect_collections_from_expr(
    expr: &ExpressionNode<'_>,
    jsonable: &HashSet<String>,
    out: &mut BTreeSet<CollectionSpec>,
    locals: &[(String, Type)],
) {
    match expr {
        ExpressionNode::MethodCall(receiver, method, type_args, args) => {
            collect_collections_from_expr(receiver, jsonable, out, locals);
            for arg in args {
                collect_collections_from_expr(arg, jsonable, out, locals);
            }
            collect_json_codec_call(
                receiver,
                &method.text,
                type_args,
                args,
                jsonable,
                locals,
                out,
            );
        }
        ExpressionNode::FunctionCall(_, _, args) => {
            for arg in args {
                collect_collections_from_expr(arg, jsonable, out, locals);
            }
        }
        ExpressionNode::Call(callee, _, args) => {
            collect_collections_from_expr(callee, jsonable, out, locals);
            for arg in args {
                collect_collections_from_expr(arg, jsonable, out, locals);
            }
        }
        ExpressionNode::Binary(a, _, b) => {
            collect_collections_from_expr(a, jsonable, out, locals);
            collect_collections_from_expr(b, jsonable, out, locals);
        }
        ExpressionNode::Unary(_, a) => collect_collections_from_expr(a, jsonable, out, locals),
        ExpressionNode::IncDec { target, .. } => {
            collect_collections_from_expr(target, jsonable, out, locals)
        }
        ExpressionNode::Parenthesized(_, a) => {
            collect_collections_from_expr(a, jsonable, out, locals)
        }
        ExpressionNode::IndexAccess(a, b) => {
            collect_collections_from_expr(a, jsonable, out, locals);
            collect_collections_from_expr(b, jsonable, out, locals);
        }
        ExpressionNode::Cast(_, _, a) => collect_collections_from_expr(a, jsonable, out, locals),
        ExpressionNode::MemberAccess(a, _) => {
            collect_collections_from_expr(a, jsonable, out, locals)
        }
        ExpressionNode::IsExpression(a, _, _) | ExpressionNode::TypeOf(_, a) => {
            collect_collections_from_expr(a, jsonable, out, locals)
        }
        ExpressionNode::Ternary(a, b, c) => {
            collect_collections_from_expr(a, jsonable, out, locals);
            collect_collections_from_expr(b, jsonable, out, locals);
            collect_collections_from_expr(c, jsonable, out, locals);
        }
        ExpressionNode::Await(_, a) => collect_collections_from_expr(a, jsonable, out, locals),
        ExpressionNode::Switch(_, a, arms) => {
            collect_collections_from_expr(a, jsonable, out, locals);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    collect_collections_from_expr(guard, jsonable, out, locals);
                }
                match &arm.body {
                    dream_syntax::nodes::expression::SwitchArmBody::Expr(e) => {
                        collect_collections_from_expr(e, jsonable, out, locals);
                    }
                    dream_syntax::nodes::expression::SwitchArmBody::Block(stmts) => {
                        let mut nested = locals.to_vec();
                        for stmt in *stmts {
                            collect_collections_from_stmts(stmt, jsonable, out, &mut nested);
                        }
                    }
                }
            }
        }
        ExpressionNode::Try(a) => collect_collections_from_expr(a, jsonable, out, locals),
        ExpressionNode::Lambda(lambda) => match &lambda.body {
            dream_syntax::nodes::expression::LambdaBody::Expr(e) => {
                collect_collections_from_expr(e, jsonable, out, locals);
            }
            dream_syntax::nodes::expression::LambdaBody::Block(stmts) => {
                let mut nested = locals.to_vec();
                for stmt in *stmts {
                    collect_collections_from_stmts(stmt, jsonable, out, &mut nested);
                }
            }
        },
        ExpressionNode::NamedArg(_, a) => collect_collections_from_expr(a, jsonable, out, locals),
        ExpressionNode::RefArgument(_, a) => {
            collect_collections_from_expr(a, jsonable, out, locals)
        }
        ExpressionNode::SyntaxBlock(block) => {
            for part in &block.parts {
                if let dream_syntax::nodes::expression::SyntaxBlockPart::Splice(e) = part {
                    collect_collections_from_expr(e, jsonable, out, locals);
                }
            }
        }
        ExpressionNode::ArrayLiteral(_, elems)
        | ExpressionNode::TupleLiteral(_, elems)
        | ExpressionNode::SetLiteral(_, elems) => {
            for elem in elems {
                collect_collections_from_expr(elem, jsonable, out, locals);
            }
        }
        ExpressionNode::ArrayRepeat(_, v, n) => {
            collect_collections_from_expr(v, jsonable, out, locals);
            collect_collections_from_expr(n, jsonable, out, locals);
        }
        ExpressionNode::MapLiteral(_, pairs) => {
            for (k, v) in pairs {
                collect_collections_from_expr(k, jsonable, out, locals);
                collect_collections_from_expr(v, jsonable, out, locals);
            }
        }
        ExpressionNode::Literal(_)
        | ExpressionNode::Identifier(_)
        | ExpressionNode::SizeOf(_, _)
        | ExpressionNode::NameOf(_, _) => {}
    }
}

pub(super) fn is_json_static_receiver(expr: &ExpressionNode<'_>) -> bool {
    matches!(expr, ExpressionNode::Identifier(tok) if tok.text == "Json")
}
