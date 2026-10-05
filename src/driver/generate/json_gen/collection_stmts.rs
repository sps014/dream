use super::collection_expr::collect_collections_from_expr;
use super::collection_expr::collect_json_codec_call;
use super::collection_expr::infer_expr_type;
use super::collection_types::CollectionSpec;
use dream_syntax::nodes::Type;
use std::collections::BTreeSet;
use std::collections::HashSet;

pub(super) fn bind_local(locals: &mut Vec<(String, Type)>, name: &str, ty: Type) {
    locals.push((name.to_string(), ty));
}

pub(super) fn collect_collections_from_stmts(
    stmt: &dream_syntax::nodes::StatementNode<'_>,
    jsonable: &HashSet<String>,
    out: &mut BTreeSet<CollectionSpec>,
    locals: &mut Vec<(String, Type)>,
) {
    use dream_syntax::nodes::StatementNode;
    match stmt {
        StatementNode::ExpressionStatement(expr) | StatementNode::AwaitStmt(expr) => {
            collect_collections_from_expr(expr, jsonable, out, locals)
        }
        StatementNode::Declaration(name, ty, init, _) => {
            collect_collections_from_expr(init, jsonable, out, locals);
            if let Some(t) = ty {
                bind_local(locals, &name.text, t.clone());
            } else if let Some(t) = infer_expr_type(init, locals) {
                bind_local(locals, &name.text, t);
            }
        }
        StatementNode::TupleDeclaration { init, .. } => {
            collect_collections_from_expr(init, jsonable, out, locals);
        }
        StatementNode::Return(expr) => {
            if let Some(e) = expr {
                collect_collections_from_expr(e, jsonable, out, locals);
            }
        }
        StatementNode::IfElse(cond, then_body, else_ifs, else_body) => {
            collect_collections_from_expr(cond, jsonable, out, locals);
            let mark = locals.len();
            for s in *then_body {
                collect_collections_from_stmts(s, jsonable, out, locals);
            }
            locals.truncate(mark);
            for (c, body) in else_ifs {
                collect_collections_from_expr(c, jsonable, out, locals);
                let mark = locals.len();
                for s in *body {
                    collect_collections_from_stmts(s, jsonable, out, locals);
                }
                locals.truncate(mark);
            }
            if let Some(body) = else_body {
                let mark = locals.len();
                for s in *body {
                    collect_collections_from_stmts(s, jsonable, out, locals);
                }
                locals.truncate(mark);
            }
        }
        StatementNode::While(cond, body) | StatementNode::DoWhile(body, cond) => {
            collect_collections_from_expr(cond, jsonable, out, locals);
            let mark = locals.len();
            for s in *body {
                collect_collections_from_stmts(s, jsonable, out, locals);
            }
            locals.truncate(mark);
        }
        StatementNode::For(init, cond, step, body) => {
            let mark = locals.len();
            if let Some(i) = init {
                collect_collections_from_stmts(i, jsonable, out, locals);
            }
            if let Some(c) = cond {
                collect_collections_from_expr(c, jsonable, out, locals);
            }
            if let Some(s) = step {
                collect_collections_from_stmts(s, jsonable, out, locals);
            }
            for st in *body {
                collect_collections_from_stmts(st, jsonable, out, locals);
            }
            locals.truncate(mark);
        }
        StatementNode::ForEach(_, iterable, _, _, body) => {
            collect_collections_from_expr(iterable, jsonable, out, locals);
            let mark = locals.len();
            for s in *body {
                collect_collections_from_stmts(s, jsonable, out, locals);
            }
            locals.truncate(mark);
        }
        StatementNode::Labeled(_, inner) => {
            collect_collections_from_stmts(inner, jsonable, out, locals)
        }
        StatementNode::Switch(subject, arms, default_body) => {
            collect_collections_from_expr(subject, jsonable, out, locals);
            for (labels, body) in arms {
                for label in labels {
                    collect_collections_from_expr(label, jsonable, out, locals);
                }
                let mark = locals.len();
                for s in *body {
                    collect_collections_from_stmts(s, jsonable, out, locals);
                }
                locals.truncate(mark);
            }
            if let Some(body) = default_body {
                let mark = locals.len();
                for s in *body {
                    collect_collections_from_stmts(s, jsonable, out, locals);
                }
                locals.truncate(mark);
            }
        }
        StatementNode::Lock(target, body) => {
            collect_collections_from_expr(target, jsonable, out, locals);
            let mark = locals.len();
            for s in *body {
                collect_collections_from_stmts(s, jsonable, out, locals);
            }
            locals.truncate(mark);
        }
        StatementNode::Overflow(_, _, body) => {
            let mark = locals.len();
            for s in *body {
                collect_collections_from_stmts(s, jsonable, out, locals);
            }
            locals.truncate(mark);
        }
        StatementNode::Defer(budget, body) => {
            if let Some(q) = budget {
                collect_collections_from_expr(q, jsonable, out, locals);
            }
            let mark = locals.len();
            for s in *body {
                collect_collections_from_stmts(s, jsonable, out, locals);
            }
            locals.truncate(mark);
        }
        StatementNode::Assignment(_, rhs) | StatementNode::MemberAssignment(_, _, rhs) => {
            collect_collections_from_expr(rhs, jsonable, out, locals);
        }
        StatementNode::IndexAssignment(a, b, rhs) => {
            collect_collections_from_expr(a, jsonable, out, locals);
            collect_collections_from_expr(b, jsonable, out, locals);
            collect_collections_from_expr(rhs, jsonable, out, locals);
        }
        StatementNode::FunctionInvocation(_, _, args) => {
            for arg in args {
                collect_collections_from_expr(arg, jsonable, out, locals);
            }
        }
        StatementNode::MethodInvocation(receiver, method, type_args, args) => {
            collect_json_codec_call(
                receiver,
                &method.text,
                type_args,
                args,
                jsonable,
                locals,
                out,
            );
            for arg in args {
                collect_collections_from_expr(arg, jsonable, out, locals);
            }
        }
        StatementNode::Break(_)
        | StatementNode::Continue(_)
        | StatementNode::WorkgroupDecl(_, _, _) => {}
    }
}
