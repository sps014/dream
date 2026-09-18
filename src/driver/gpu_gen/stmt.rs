//! Statement → WGSL lowering.

use super::context::EmitCtx;
use super::expr::{coerce_expr_to_wgsl_ty, emit_call, emit_expr};
use super::ident::escape_wgsl_ident;
use super::ty::{dream_ty_to_wgsl, infer_wgsl_ty};
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
        StatementNode::TupleDeclaration { pattern, init, .. } => {
            pattern.position().or_else(|| init.position())
        }
        StatementNode::IndexAssignment(arr, _, _) => arr.position(),
        StatementNode::Return(Some(e))
        | StatementNode::ExpressionStatement(e)
        | StatementNode::AwaitStmt(e)
        | StatementNode::While(e, _)
        | StatementNode::DoWhile(_, e)
        | StatementNode::Lock(e, _)
        | StatementNode::IfElse(e, _, _, _)
        | StatementNode::Switch(e, _, _) => e.position(),
        StatementNode::Defer(Some(e), _) => e.position(),
        StatementNode::Defer(None, _) => None,
        StatementNode::For(_, Some(cond), _, _) => cond.position(),
        StatementNode::Labeled(_, inner) => stmt_span(inner),
        StatementNode::Return(None)
        | StatementNode::For(_, None, _, _)
        | StatementNode::Break(_)
        | StatementNode::Continue(_) => None,
    }
}

/// True when `stmts` contain a `break` that belongs to a loop enclosing them.
///
/// Dream's `switch` does not capture `break` (it has no fall-through, so a `break` in a case body
/// targets the enclosing loop), but WGSL's `switch` does. Nested loops are skipped because their
/// own `break` binds to them; nested switches are not, for the same reason on the Dream side.
fn breaks_enclosing_loop(stmts: &[StatementNode<'_>]) -> bool {
    stmts.iter().any(|s| match s {
        StatementNode::Break(_) => true,
        StatementNode::IfElse(_, then_b, elifs, else_b) => {
            breaks_enclosing_loop(then_b)
                || elifs.iter().any(|(_, b)| breaks_enclosing_loop(b))
                || else_b.is_some_and(breaks_enclosing_loop)
        }
        StatementNode::Switch(_, cases, default) => {
            cases.iter().any(|(_, b)| breaks_enclosing_loop(b))
                || default.is_some_and(breaks_enclosing_loop)
        }
        StatementNode::Labeled(_, inner) => breaks_enclosing_loop(std::slice::from_ref(*inner)),
        _ => false,
    })
}

fn reject_label(stmt: &StatementNode<'_>, keyword: &str, label: &str, ctx: &EmitCtx<'_>) {
    ctx.report_error(
        format!(
            "GPU shader '{}' cannot use '{keyword} {label}': WGSL has no loop labels, so \
             {keyword} always applies to the innermost loop. Restructure with a flag local, or \
             move the inner loop into a @gpu helper and return from it",
            ctx.kernel
        ),
        stmt_span(stmt),
    );
}

pub(super) fn emit_stmts(
    stmts: &[StatementNode<'_>],
    out: &mut String,
    wg: &mut String,
    indent: usize,
    ctx: &EmitCtx<'_>,
) {
    ctx.push_scope();
    for s in stmts {
        emit_stmt(s, out, wg, indent, ctx);
    }
    ctx.pop_scope();
}

/// Report `nameof(...)` / `typeof(...)` in GPU shader bodies (`string` is illegal in WGSL).
pub(super) fn reject_gpu_string_meta(stmts: &[StatementNode<'_>], ctx: &EmitCtx<'_>) {
    for s in stmts {
        scan_stmt_string_meta(s, ctx);
    }
}

fn scan_stmt_string_meta(stmt: &StatementNode<'_>, ctx: &EmitCtx<'_>) {
    match stmt {
        StatementNode::ExpressionStatement(e)
        | StatementNode::AwaitStmt(e)
        | StatementNode::Return(Some(e))
        | StatementNode::Assignment(_, e)
        | StatementNode::Declaration(_, _, e, _)
        | StatementNode::TupleDeclaration { init: e, .. } => scan_expr_string_meta(e, ctx),
        StatementNode::IndexAssignment(a, i, v) => {
            scan_expr_string_meta(a, ctx);
            scan_expr_string_meta(i, ctx);
            scan_expr_string_meta(v, ctx);
        }
        StatementNode::MemberAssignment(r, _, v) => {
            scan_expr_string_meta(r, ctx);
            scan_expr_string_meta(v, ctx);
        }
        StatementNode::FunctionInvocation(_, _, args) => {
            for a in args {
                scan_expr_string_meta(a, ctx);
            }
        }
        StatementNode::MethodInvocation(r, _, _, args) => {
            scan_expr_string_meta(r, ctx);
            for a in args {
                scan_expr_string_meta(a, ctx);
            }
        }
        StatementNode::IfElse(cond, then_b, elifs, else_b) => {
            scan_expr_string_meta(cond, ctx);
            reject_gpu_string_meta(then_b, ctx);
            for (c, body) in elifs {
                scan_expr_string_meta(c, ctx);
                reject_gpu_string_meta(body, ctx);
            }
            if let Some(eb) = else_b {
                reject_gpu_string_meta(eb, ctx);
            }
        }
        StatementNode::While(cond, body) => {
            scan_expr_string_meta(cond, ctx);
            reject_gpu_string_meta(body, ctx);
        }
        StatementNode::DoWhile(body, cond) => {
            reject_gpu_string_meta(body, ctx);
            scan_expr_string_meta(cond, ctx);
        }
        StatementNode::For(init, cond, step, body) => {
            if let Some(i) = init {
                scan_stmt_string_meta(i, ctx);
            }
            if let Some(c) = cond {
                scan_expr_string_meta(c, ctx);
            }
            if let Some(s) = step {
                scan_stmt_string_meta(s, ctx);
            }
            reject_gpu_string_meta(body, ctx);
        }
        StatementNode::Switch(subj, cases, default) => {
            scan_expr_string_meta(subj, ctx);
            for (labels, body) in cases {
                for lit in labels {
                    scan_expr_string_meta(lit, ctx);
                }
                reject_gpu_string_meta(body, ctx);
            }
            if let Some(db) = default {
                reject_gpu_string_meta(db, ctx);
            }
        }
        StatementNode::Lock(e, body) => {
            scan_expr_string_meta(e, ctx);
            reject_gpu_string_meta(body, ctx);
        }
        StatementNode::Defer(budget, body) => {
            if let Some(q) = budget {
                scan_expr_string_meta(q, ctx);
            }
            reject_gpu_string_meta(body, ctx);
        }
        StatementNode::ForEach(_, e, _, _, body) => {
            scan_expr_string_meta(e, ctx);
            reject_gpu_string_meta(body, ctx);
        }
        StatementNode::Labeled(_, inner) => scan_stmt_string_meta(inner, ctx),
        StatementNode::WorkgroupDecl(..)
        | StatementNode::Return(None)
        | StatementNode::Break(_)
        | StatementNode::Continue(_) => {}
    }
}

fn scan_expr_string_meta(expr: &ExpressionNode<'_>, ctx: &EmitCtx<'_>) {
    match expr {
        ExpressionNode::NameOf(tok, _) => {
            ctx.report_error(
                format!(
                    "GPU shader '{}' cannot use nameof(...); nameof yields string, which is not allowed in shaders — keep it on the CPU host",
                    ctx.kernel
                ),
                Some(tok.position),
            );
        }
        ExpressionNode::TypeOf(tok, _) => {
            ctx.report_error(
                format!(
                    "GPU shader '{}' cannot use typeof(...); typeof yields string, which is not allowed in shaders — keep it on the CPU host",
                    ctx.kernel
                ),
                Some(tok.position),
            );
        }
        ExpressionNode::Binary(l, _, r) | ExpressionNode::IndexAccess(l, r) => {
            scan_expr_string_meta(l, ctx);
            scan_expr_string_meta(r, ctx);
        }
        ExpressionNode::Ternary(c, t, e) => {
            scan_expr_string_meta(c, ctx);
            scan_expr_string_meta(t, ctx);
            scan_expr_string_meta(e, ctx);
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
        | ExpressionNode::RefArgument(_, e) => scan_expr_string_meta(e, ctx),
        ExpressionNode::FunctionCall(_, _, args)
        | ExpressionNode::ArrayLiteral(_, args)
        | ExpressionNode::TupleLiteral(_, args)
        | ExpressionNode::SetLiteral(_, args) => {
            for a in args {
                scan_expr_string_meta(a, ctx);
            }
        }
        ExpressionNode::Call(c, _, args) | ExpressionNode::MethodCall(c, _, _, args) => {
            scan_expr_string_meta(c, ctx);
            for a in args {
                scan_expr_string_meta(a, ctx);
            }
        }
        ExpressionNode::ArrayRepeat(_, v, n) => {
            scan_expr_string_meta(v, ctx);
            scan_expr_string_meta(n, ctx);
        }
        ExpressionNode::MapLiteral(_, entries) => {
            for (k, v) in entries {
                scan_expr_string_meta(k, ctx);
                scan_expr_string_meta(v, ctx);
            }
        }
        ExpressionNode::Switch(_, subj, arms) => {
            scan_expr_string_meta(subj, ctx);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    scan_expr_string_meta(g, ctx);
                }
                match &arm.body {
                    dream_syntax::nodes::SwitchArmBody::Expr(e) => scan_expr_string_meta(e, ctx),
                    dream_syntax::nodes::SwitchArmBody::Block(stmts) => {
                        reject_gpu_string_meta(stmts, ctx)
                    }
                }
            }
        }
        ExpressionNode::Lambda(l) => match &l.body {
            dream_syntax::nodes::LambdaBody::Expr(e) => scan_expr_string_meta(e, ctx),
            dream_syntax::nodes::LambdaBody::Block(stmts) => reject_gpu_string_meta(stmts, ctx),
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
            // The store is converted to the field's own type. WGSL only converts abstract
            // literals implicitly, so a concrete mismatch (an `int` expression into a `float`
            // field, or the `u32`-typed sample_mask builtin) is an error there without this.
            let want = ctx
                .lookup_struct_field(&infer_wgsl_ty(obj, ctx), &member.text)
                .unwrap_or_default();
            out.push_str(&format!(
                "{}{}.{} = {};\n",
                p,
                emit_expr(obj, ctx),
                escape_wgsl_ident(&member.text),
                coerce_expr_to_wgsl_ty(value, &want, ctx)
            ));
        }
        StatementNode::Return(None) => out.push_str(&format!("{}return;\n", p)),
        StatementNode::Return(Some(e)) => {
            let rhs = emit_expr(e, ctx);
            out.push_str(&format!("{}return {};\n", p, rhs));
        }
        StatementNode::IfElse(cond, then_b, elifs, else_b) => {
            out.push_str(&format!("{}if ({}) {{\n", p, emit_expr(cond, ctx)));
            emit_stmts(then_b, out, wg, indent + 1, ctx);
            out.push_str(&format!("{}}}\n", p));
            for (c, body) in elifs {
                out.push_str(&format!("{}else if ({}) {{\n", p, emit_expr(c, ctx)));
                emit_stmts(body, out, wg, indent + 1, ctx);
                out.push_str(&format!("{}}}\n", p));
            }
            if let Some(eb) = else_b {
                out.push_str(&format!("{}else {{\n", p));
                emit_stmts(eb, out, wg, indent + 1, ctx);
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
            emit_stmts(body, out, wg, indent + 1, ctx);
            out.push_str(&format!("{}}}\n", p));
        }
        StatementNode::DoWhile(body, cond) => {
            // The test goes in `continuing` rather than at the end of the body, so `continue`
            // reaches it. Falling out of the body arrives there just the same.
            out.push_str(&format!("{}loop {{\n", p));
            emit_stmts(body, out, wg, indent + 1, ctx);
            out.push_str(&format!(
                "{}  continuing {{ break if !({}); }}\n",
                p,
                emit_expr(cond, ctx)
            ));
            out.push_str(&format!("{}}}\n", p));
        }
        StatementNode::For(init, cond, step, body) => {
            if let Some(i) = init {
                emit_stmt(i, out, wg, indent, ctx);
            }
            out.push_str(&format!("{}loop {{\n", p));
            if let Some(c) = cond {
                out.push_str(&format!(
                    "{}  if (!({})) {{ break; }}\n",
                    p,
                    emit_expr(c, ctx)
                ));
            }
            emit_stmts(body, out, wg, indent + 1, ctx);
            if let Some(s) = step {
                // WGSL `continue` re-enters at `continuing`, so a step left at the end of the
                // body would be skipped by `continue` and the loop would never advance.
                out.push_str(&format!("{}  continuing {{\n", p));
                emit_stmt(s, out, wg, indent + 2, ctx);
                out.push_str(&format!("{}  }}\n", p));
            }
            out.push_str(&format!("{}}}\n", p));
        }
        // WGSL has no loop labels, and `break`/`continue` always target the innermost loop. A
        // labelled one naming an outer loop used to emit the unlabelled form, which silently ran
        // the wrong loop; rejecting it is the honest answer until the flag-variable lowering
        // lands.
        StatementNode::Break(label) => match label {
            Some(name) => reject_label(stmt, "break", name, ctx),
            None => out.push_str(&format!("{}break;\n", p)),
        },
        StatementNode::Continue(label) => match label {
            Some(name) => reject_label(stmt, "continue", name, ctx),
            None => out.push_str(&format!("{}continue;\n", p)),
        },
        StatementNode::Labeled(_, inner) => emit_stmt(inner, out, wg, indent, ctx),
        StatementNode::Switch(subject, cases, default) => {
            // The subject is bound to a `let` so it is evaluated once: it is compared against
            // every label, and splicing the expression into each comparison would re-run any
            // call or buffer read inside it.
            let sub = emit_expr(subject, ctx);
            let subject_ty = infer_wgsl_ty(subject, ctx);
            let scrutinee = "dream_sw";
            out.push_str(&format!("{}{{\n", p));
            out.push_str(&format!("{}  let {scrutinee} = {sub};\n", p));

            // WGSL `switch` needs an integer selector and, because it captures `break`, case
            // bodies that do not `break` an enclosing loop. Anything else keeps the if-else
            // chain, which is equivalent but does not fold to a jump table.
            let native = matches!(subject_ty.as_str(), "i32" | "u32")
                && !cases.iter().any(|(_, b)| breaks_enclosing_loop(b))
                && !default.is_some_and(breaks_enclosing_loop);

            if native {
                out.push_str(&format!("{}  switch ({scrutinee}) {{\n", p));
                for (labels, body) in cases {
                    let sel: Vec<String> = labels.iter().map(|l| emit_expr(l, ctx)).collect();
                    out.push_str(&format!("{}    case {}: {{\n", p, sel.join(", ")));
                    emit_stmts(body, out, wg, indent + 3, ctx);
                    out.push_str(&format!("{}    }}\n", p));
                }
                // WGSL requires exactly one default clause even when Dream omits it.
                out.push_str(&format!("{}    default: {{\n", p));
                if let Some(db) = default {
                    emit_stmts(db, out, wg, indent + 3, ctx);
                }
                out.push_str(&format!("{}    }}\n", p));
                out.push_str(&format!("{}  }}\n", p));
            } else {
                let mut first = true;
                for (labels, body) in cases {
                    let conds: Vec<String> = labels
                        .iter()
                        .map(|l| format!("({scrutinee}) == ({})", emit_expr(l, ctx)))
                        .collect();
                    let kw = if first { "if" } else { "else if" };
                    first = false;
                    out.push_str(&format!("{}  {} ({}) {{\n", p, kw, conds.join(" || ")));
                    emit_stmts(body, out, wg, indent + 2, ctx);
                    out.push_str(&format!("{}  }}\n", p));
                }
                if let Some(db) = default {
                    let kw = if first { "if (true)" } else { "else" };
                    out.push_str(&format!("{}  {kw} {{\n", p));
                    emit_stmts(db, out, wg, indent + 2, ctx);
                    out.push_str(&format!("{}  }}\n", p));
                }
            }
            out.push_str(&format!("{}}}\n", p));
        }
        StatementNode::FunctionInvocation(name, type_args, args)
        | StatementNode::MethodInvocation(_, name, type_args, args) => {
            if type_args.as_ref().is_some_and(|a| !a.is_empty()) {
                ctx.report_error(
                    format!(
                        "GPU shader '{}' does not support generic type arguments on calls",
                        ctx.kernel
                    ),
                    Some(name.position),
                );
            }
            let call = emit_call(&name.text, args, ctx);
            out.push_str(&format!("{}{};\n", p, call));
        }
        StatementNode::ExpressionStatement(e) => {
            if let ExpressionNode::IncDec { is_inc, target, .. } = e {
                let place = emit_expr(target, ctx);
                let op = if *is_inc { "+" } else { "-" };
                out.push_str(&format!("{}{} = {} {} 1;\n", p, place, place, op));
                return;
            }
            out.push_str(&format!("{}{};\n", p, emit_expr(e, ctx)));
        }
        StatementNode::ForEach(..)
        | StatementNode::AwaitStmt(_)
        | StatementNode::Lock(..)
        | StatementNode::Defer(..)
        | StatementNode::TupleDeclaration { .. } => {
            let kind = match stmt {
                StatementNode::ForEach(..) => "for-each",
                StatementNode::AwaitStmt(_) => "await",
                StatementNode::Lock(..) => "lock",
                StatementNode::Defer(..) => "defer",
                StatementNode::TupleDeclaration { .. } => "tuple declaration",
                _ => "statement",
            };
            ctx.report_error(
                format!(
                    "GPU shader '{}' contains unsupported {kind}; remove it or rewrite with supported control flow",
                    ctx.kernel
                ),
                stmt_span(stmt),
            );
        }
    }
}
