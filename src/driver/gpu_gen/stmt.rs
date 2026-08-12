//! Statement → WGSL lowering.

use super::context::EmitCtx;
use super::expr::{coerce_expr_to_wgsl_ty, emit_call, emit_expr};
use super::ident::escape_wgsl_ident;
use super::ty::{dream_ty_to_wgsl, infer_wgsl_ty};
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::expression::ExpressionNode;
use dream_syntax::nodes::statement::StatementNode;
use dream_text::text_span::TextSpan;

fn stmt_span(stmt: &StatementNode<'_>) -> Option<TextSpan> {
    match stmt {
        StatementNode::Assignment(tok, _)
        | StatementNode::Declaration(tok, _, _, _)
        | StatementNode::WorkgroupDecl(tok, _, _)
        | StatementNode::FunctionInvocation(tok, _, _)
        | StatementNode::MethodInvocation(_, tok, _, _)
        | StatementNode::MemberAssignment(_, tok, _)
        | StatementNode::ForEach(tok, _, _, _, _) => Some(tok.position),
        StatementNode::TupleDeclaration { pattern, init, .. } => pattern
            .position()
            .or_else(|| init.position()),
        StatementNode::IndexAssignment(arr, _, _) => arr.position(),
        StatementNode::Return(Some(e))
        | StatementNode::ExpressionStatement(e)
        | StatementNode::AwaitStmt(e)
        | StatementNode::While(e, _)
        | StatementNode::DoWhile(_, e)
        | StatementNode::Lock(e, _)
        | StatementNode::IfElse(e, _, _, _)
        | StatementNode::Switch(e, _, _) => e.position(),
        StatementNode::For(_, Some(cond), _, _) => cond.position(),
        StatementNode::Labeled(_, inner) => stmt_span(inner),
        StatementNode::Return(None)
        | StatementNode::For(_, None, _, _)
        | StatementNode::Break(_)
        | StatementNode::Continue(_) => None,
    }
}

pub(super) fn emit_stmts(
    stmts: &[StatementNode<'_>],
    out: &mut String,
    wg: &mut String,
    indent: usize,
    ctx: &EmitCtx<'_>,
    diagnostics: &mut DiagnosticBag,
    kernel: &str,
) {
    ctx.push_scope();
    for s in stmts {
        emit_stmt(s, out, wg, indent, ctx, diagnostics, kernel);
    }
    ctx.pop_scope();
}

/// Report `nameof(...)` in GPU shader bodies (`string` is illegal in WGSL).
pub(super) fn reject_gpu_nameof(
    stmts: &[StatementNode<'_>],
    diagnostics: &mut DiagnosticBag,
    kernel: &str,
) {
    for s in stmts {
        scan_stmt_nameof(s, diagnostics, kernel);
    }
}

fn scan_stmt_nameof(stmt: &StatementNode<'_>, diagnostics: &mut DiagnosticBag, kernel: &str) {
    match stmt {
        StatementNode::ExpressionStatement(e)
        | StatementNode::AwaitStmt(e)
        | StatementNode::Return(Some(e))
        | StatementNode::Assignment(_, e)
        | StatementNode::Declaration(_, _, e, _)
        | StatementNode::TupleDeclaration { init: e, .. } => {
            scan_expr_nameof(e, diagnostics, kernel)
        }
        StatementNode::IndexAssignment(a, i, v) => {
            scan_expr_nameof(a, diagnostics, kernel);
            scan_expr_nameof(i, diagnostics, kernel);
            scan_expr_nameof(v, diagnostics, kernel);
        }
        StatementNode::MemberAssignment(r, _, v) => {
            scan_expr_nameof(r, diagnostics, kernel);
            scan_expr_nameof(v, diagnostics, kernel);
        }
        StatementNode::FunctionInvocation(_, _, args) => {
            for a in args {
                scan_expr_nameof(a, diagnostics, kernel);
            }
        }
        StatementNode::MethodInvocation(r, _, _, args) => {
            scan_expr_nameof(r, diagnostics, kernel);
            for a in args {
                scan_expr_nameof(a, diagnostics, kernel);
            }
        }
        StatementNode::IfElse(cond, then_b, elifs, else_b) => {
            scan_expr_nameof(cond, diagnostics, kernel);
            reject_gpu_nameof(then_b, diagnostics, kernel);
            for (c, body) in elifs {
                scan_expr_nameof(c, diagnostics, kernel);
                reject_gpu_nameof(body, diagnostics, kernel);
            }
            if let Some(eb) = else_b {
                reject_gpu_nameof(eb, diagnostics, kernel);
            }
        }
        StatementNode::While(cond, body) => {
            scan_expr_nameof(cond, diagnostics, kernel);
            reject_gpu_nameof(body, diagnostics, kernel);
        }
        StatementNode::DoWhile(body, cond) => {
            reject_gpu_nameof(body, diagnostics, kernel);
            scan_expr_nameof(cond, diagnostics, kernel);
        }
        StatementNode::For(init, cond, step, body) => {
            if let Some(i) = init {
                scan_stmt_nameof(i, diagnostics, kernel);
            }
            if let Some(c) = cond {
                scan_expr_nameof(c, diagnostics, kernel);
            }
            if let Some(s) = step {
                scan_stmt_nameof(s, diagnostics, kernel);
            }
            reject_gpu_nameof(body, diagnostics, kernel);
        }
        StatementNode::Switch(subj, cases, default) => {
            scan_expr_nameof(subj, diagnostics, kernel);
            for (labels, body) in cases {
                for lit in labels {
                    scan_expr_nameof(lit, diagnostics, kernel);
                }
                reject_gpu_nameof(body, diagnostics, kernel);
            }
            if let Some(db) = default {
                reject_gpu_nameof(db, diagnostics, kernel);
            }
        }
        StatementNode::Lock(e, body) => {
            scan_expr_nameof(e, diagnostics, kernel);
            reject_gpu_nameof(body, diagnostics, kernel);
        }
        StatementNode::ForEach(_, e, _, _, body) => {
            scan_expr_nameof(e, diagnostics, kernel);
            reject_gpu_nameof(body, diagnostics, kernel);
        }
        StatementNode::Labeled(_, inner) => scan_stmt_nameof(inner, diagnostics, kernel),
        StatementNode::WorkgroupDecl(..)
        | StatementNode::Return(None)
        | StatementNode::Break(_)
        | StatementNode::Continue(_) => {}
    }
}

fn scan_expr_nameof(expr: &ExpressionNode<'_>, diagnostics: &mut DiagnosticBag, kernel: &str) {
    match expr {
        ExpressionNode::NameOf(tok, _) => {
            diagnostics.report_error(
                format!(
                    "GPU shader '{kernel}' cannot use nameof(...); nameof yields string, which is not allowed in shaders — keep it on the CPU host"
                ),
                Some(tok.position),
            );
        }
        ExpressionNode::Binary(l, _, r) | ExpressionNode::IndexAccess(l, r) => {
            scan_expr_nameof(l, diagnostics, kernel);
            scan_expr_nameof(r, diagnostics, kernel);
        }
        ExpressionNode::Ternary(c, t, e) => {
            scan_expr_nameof(c, diagnostics, kernel);
            scan_expr_nameof(t, diagnostics, kernel);
            scan_expr_nameof(e, diagnostics, kernel);
        }
        ExpressionNode::Unary(_, e)
        | ExpressionNode::IncDec { target: e, .. }
        | ExpressionNode::Parenthesized(_, e)
        | ExpressionNode::Cast(_, _, e)
        | ExpressionNode::IsExpression(e, _, _)
        | ExpressionNode::MemberAccess(e, _)
        | ExpressionNode::Await(_, e)
        | ExpressionNode::Try(e)
        | ExpressionNode::NamedArg(_, e)
        | ExpressionNode::RefArgument(_, e) => scan_expr_nameof(e, diagnostics, kernel),
        ExpressionNode::FunctionCall(_, _, args)
        | ExpressionNode::ArrayLiteral(_, args)
        | ExpressionNode::TupleLiteral(_, args)
        | ExpressionNode::SetLiteral(_, args) => {
            for a in args {
                scan_expr_nameof(a, diagnostics, kernel);
            }
        }
        ExpressionNode::Call(c, _, args) | ExpressionNode::MethodCall(c, _, _, args) => {
            scan_expr_nameof(c, diagnostics, kernel);
            for a in args {
                scan_expr_nameof(a, diagnostics, kernel);
            }
        }
        ExpressionNode::MapLiteral(_, entries) => {
            for (k, v) in entries {
                scan_expr_nameof(k, diagnostics, kernel);
                scan_expr_nameof(v, diagnostics, kernel);
            }
        }
        ExpressionNode::Switch(_, subj, arms) => {
            scan_expr_nameof(subj, diagnostics, kernel);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    scan_expr_nameof(g, diagnostics, kernel);
                }
                match &arm.body {
                    dream_syntax::nodes::SwitchArmBody::Expr(e) => {
                        scan_expr_nameof(e, diagnostics, kernel)
                    }
                    dream_syntax::nodes::SwitchArmBody::Block(stmts) => {
                        reject_gpu_nameof(stmts, diagnostics, kernel)
                    }
                }
            }
        }
        ExpressionNode::Lambda(l) => match &l.body {
            dream_syntax::nodes::LambdaBody::Expr(e) => scan_expr_nameof(e, diagnostics, kernel),
            dream_syntax::nodes::LambdaBody::Block(stmts) => {
                reject_gpu_nameof(stmts, diagnostics, kernel)
            }
        },
        ExpressionNode::Literal(_)
        | ExpressionNode::Identifier(_)
        | ExpressionNode::SizeOf(_, _)
        | ExpressionNode::SyntaxBlock(_) => {}
    }
}

fn pad(n: usize) -> String {
    "  ".repeat(n)
}

fn emit_stmt(
    stmt: &StatementNode<'_>,
    out: &mut String,
    wg: &mut String,
    indent: usize,
    ctx: &EmitCtx<'_>,
    diagnostics: &mut DiagnosticBag,
    kernel: &str,
) {
    let p = pad(indent);
    match stmt {
        StatementNode::WorkgroupDecl(name, ty, size) => {
            let elem = dream_ty_to_wgsl(ty);
            wg.push_str(&format!(
                "var<workgroup> {}: array<{}, {}>;\n",
                ctx.mangle(&name.text),
                elem,
                size
            ));
        }
        StatementNode::Declaration(name, ty, init, _) => {
            // WGSL forbids `var _` / `let _`; use a phony assignment instead.
            if name.text == "_" {
                if let ExpressionNode::IncDec {
                    prefix,
                    is_inc,
                    target,
                    ..
                } = init
                {
                    let place = emit_expr(target, ctx);
                    let op = if *is_inc { "+" } else { "-" };
                    if *prefix {
                        out.push_str(&format!("{}{} = {} {} 1;\n", p, place, place, op));
                        out.push_str(&format!("{}{} = {};\n", p, "_", place));
                    } else {
                        out.push_str(&format!("{}{} = {};\n", p, "_", place));
                        out.push_str(&format!("{}{} = {} {} 1;\n", p, place, place, op));
                    }
                    return;
                }
                out.push_str(&format!("{}{} = {};\n", p, "_", emit_expr(init, ctx)));
                return;
            }
            if let ExpressionNode::IncDec {
                prefix,
                is_inc,
                target,
                ..
            } = init
            {
                let place = emit_expr(target, ctx);
                let op = if *is_inc { "+" } else { "-" };
                let t = ty
                    .as_ref()
                    .map(dream_ty_to_wgsl)
                    .unwrap_or_else(|| infer_wgsl_ty(init, ctx));
                ctx.define_local(&name.text, t.clone());
                let wname = escape_wgsl_ident(&name.text);
                if *prefix {
                    out.push_str(&format!("{}{} = {} {} 1;\n", p, place, place, op));
                    out.push_str(&format!("{}var {}: {} = {};\n", p, wname, t, place));
                } else {
                    out.push_str(&format!("{}var {}: {} = {};\n", p, wname, t, place));
                    out.push_str(&format!("{}{} = {} {} 1;\n", p, place, place, op));
                }
                return;
            }
            let t = ty
                .as_ref()
                .map(dream_ty_to_wgsl)
                .unwrap_or_else(|| infer_wgsl_ty(init, ctx));
            let init_s = coerce_expr_to_wgsl_ty(init, &t, ctx);
            ctx.define_local(&name.text, t.clone());
            out.push_str(&format!(
                "{}var {}: {} = {};\n",
                p,
                escape_wgsl_ident(&name.text),
                t,
                init_s
            ));
        }
        StatementNode::Assignment(name, value) => {
            let lhs = ctx.rewrite_ident(&name.text);
            let want = ctx
                .lookup_local(&name.text)
                .unwrap_or_else(|| infer_wgsl_ty(value, ctx));
            let rhs = coerce_expr_to_wgsl_ty(value, &want, ctx);
            out.push_str(&format!("{}{} = {};\n", p, lhs, rhs));
        }
        StatementNode::IndexAssignment(arr, idx, value) => {
            let arr_s = emit_expr(arr, ctx);
            let idx_s = coerce_expr_to_wgsl_ty(idx, "i32", ctx);
            let elem_ty = match arr {
                ExpressionNode::Identifier(name) => {
                    if let Some(t) = ctx.lookup_local(&name.text) {
                        t.strip_prefix("array<")
                            .and_then(|s| s.strip_suffix('>'))
                            .unwrap_or(t.as_str())
                            .to_string()
                    } else if let Some(b) = ctx.binding(&name.text) {
                        b.wgsl_ty.clone()
                    } else {
                        infer_wgsl_ty(value, ctx)
                    }
                }
                _ => infer_wgsl_ty(value, ctx),
            };
            let val_s = coerce_expr_to_wgsl_ty(value, &elem_ty, ctx);
            let atomic = matches!(arr, ExpressionNode::Identifier(n) if ctx.is_atomic_buf(&n.text));
            if atomic {
                out.push_str(&format!(
                    "{}atomicStore(&{}[u32({})], {});\n",
                    p, arr_s, idx_s, val_s
                ));
            } else {
                out.push_str(&format!("{}{}[u32({})] = {};\n", p, arr_s, idx_s, val_s));
            }
        }
        StatementNode::MemberAssignment(obj, member, value) => {
            out.push_str(&format!(
                "{}{}.{} = {};\n",
                p,
                emit_expr(obj, ctx),
                escape_wgsl_ident(&member.text),
                emit_expr(value, ctx)
            ));
        }
        StatementNode::Return(None) => out.push_str(&format!("{}return;\n", p)),
        StatementNode::Return(Some(e)) => {
            let rhs = emit_expr(e, ctx);
            out.push_str(&format!("{}return {};\n", p, rhs));
        },
        StatementNode::IfElse(cond, then_b, elifs, else_b) => {
            out.push_str(&format!("{}if ({}) {{\n", p, emit_expr(cond, ctx)));
            emit_stmts(then_b, out, wg, indent + 1, ctx, diagnostics, kernel);
            out.push_str(&format!("{}}}\n", p));
            for (c, body) in elifs {
                out.push_str(&format!("{}else if ({}) {{\n", p, emit_expr(c, ctx)));
                emit_stmts(body, out, wg, indent + 1, ctx, diagnostics, kernel);
                out.push_str(&format!("{}}}\n", p));
            }
            if let Some(eb) = else_b {
                out.push_str(&format!("{}else {{\n", p));
                emit_stmts(eb, out, wg, indent + 1, ctx, diagnostics, kernel);
                out.push_str(&format!("{}}}\n", p));
            }
        }
        StatementNode::While(cond, body) => {
            out.push_str(&format!("{}loop {{\n", p));
            out.push_str(&format!(
                "{}  if (!({})) {{ break; }}\n",
                p,
                emit_expr(cond, ctx)
            ));
            emit_stmts(body, out, wg, indent + 1, ctx, diagnostics, kernel);
            out.push_str(&format!("{}}}\n", p));
        }
        StatementNode::DoWhile(body, cond) => {
            out.push_str(&format!("{}loop {{\n", p));
            emit_stmts(body, out, wg, indent + 1, ctx, diagnostics, kernel);
            out.push_str(&format!(
                "{}  if (!({})) {{ break; }}\n",
                p,
                emit_expr(cond, ctx)
            ));
            out.push_str(&format!("{}}}\n", p));
        }
        StatementNode::For(init, cond, step, body) => {
            if let Some(i) = init {
                emit_stmt(i, out, wg, indent, ctx, diagnostics, kernel);
            }
            out.push_str(&format!("{}loop {{\n", p));
            if let Some(c) = cond {
                out.push_str(&format!(
                    "{}  if (!({})) {{ break; }}\n",
                    p,
                    emit_expr(c, ctx)
                ));
            }
            emit_stmts(body, out, wg, indent + 1, ctx, diagnostics, kernel);
            if let Some(s) = step {
                emit_stmt(s, out, wg, indent + 1, ctx, diagnostics, kernel);
            }
            out.push_str(&format!("{}}}\n", p));
        }
        StatementNode::Break(_) => out.push_str(&format!("{}break;\n", p)),
        StatementNode::Continue(_) => out.push_str(&format!("{}continue;\n", p)),
        StatementNode::Labeled(_, inner) => emit_stmt(inner, out, wg, indent, ctx, diagnostics, kernel),
        StatementNode::Switch(subject, cases, default) => {
            // Lower to if-else chain (WGSL switch is more limited).
            let sub = emit_expr(subject, ctx);
            let mut first = true;
            for (labels, body) in cases {
                let conds: Vec<String> = labels
                    .iter()
                    .map(|l| format!("({}) == ({})", sub, emit_expr(l, ctx)))
                    .collect();
                let kw = if first { "if" } else { "else if" };
                first = false;
                out.push_str(&format!("{}{} ({}) {{\n", p, kw, conds.join(" || ")));
                emit_stmts(body, out, wg, indent + 1, ctx, diagnostics, kernel);
                out.push_str(&format!("{}}}\n", p));
            }
            if let Some(db) = default {
                out.push_str(&format!("{}else {{\n", p));
                emit_stmts(db, out, wg, indent + 1, ctx, diagnostics, kernel);
                out.push_str(&format!("{}}}\n", p));
            }
        }
        StatementNode::FunctionInvocation(name, _, args)
        | StatementNode::MethodInvocation(_, name, _, args) => {
            let call = emit_call(&name.text, args, ctx);
            out.push_str(&format!("{}{};\n", p, call));
        }
        StatementNode::ExpressionStatement(e) => {
            out.push_str(&format!("{}{};\n", p, emit_expr(e, ctx)));
        }
        StatementNode::ForEach(..)
        | StatementNode::AwaitStmt(_)
        | StatementNode::Lock(..)
        | StatementNode::TupleDeclaration { .. } => {
            let kind = match stmt {
                StatementNode::ForEach(..) => "for-each",
                StatementNode::AwaitStmt(_) => "await",
                StatementNode::Lock(..) => "lock",
                StatementNode::TupleDeclaration { .. } => "tuple declaration",
                _ => "statement",
            };
            diagnostics.report_error(
                format!(
                    "GPU shader '{kernel}' contains unsupported {kind}; remove it or rewrite with supported control flow"
                ),
                stmt_span(stmt),
            );
        }
    }
}
