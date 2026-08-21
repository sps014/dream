//! In-arena rewrite of `SyntaxBlock` expressions into ordinary Dream expressions.

use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;
use dream_text::line_text::LineText;
use dream_text::text_span::TextSpan;
use dream_syntax::lexer::Lexer;
use dream_syntax::nodes::function::FunctionNode;
use dream_syntax::nodes::{
    ExpressionNode, LambdaBody, StatementNode, SwitchArm, SwitchArmBody, SyntaxBlockPart,
};
use dream_syntax::parser::Parser;
use indexmap::IndexMap;
use std::io::Error;

/// Origin of one generated replacement: the `html {}` / `quote {}` block it was
/// produced from, in the user's own source file.
pub struct GenOrigin {
    pub real_file: String,
    /// Byte offset of the block's opening `{` in `real_file`.
    pub block_start: usize,
}

const WRAP_PREFIX: &str = "fun __gen(): void { return ";

/// Parses a Dream expression from `source` and returns it arena-allocated. Diagnostics
/// are remapped onto the origin block in the user's file (offset within the generated
/// source is preserved, clamped to the block).
pub fn parse_expression_source<'a>(
    arena: &'a Bump,
    source: &str,
    diagnostics: &mut DiagnosticBag,
    origin: Option<&GenOrigin>,
    real_source: Option<&str>,
) -> Result<ExpressionNode<'a>, Error> {
    // Wrap as `fun __gen(): void { return <expr>; }` and pull the return expression.
    let wrapped = format!("{}{}; }}\n", WRAP_PREFIX, source);
    let mut local = DiagnosticBag::new(Some("<syntax-replace>".into()));
    let lexer = Lexer::new(wrapped);
    let mut parser = Parser::new(lexer, arena, &mut local);
    let ast = match parser.parse() {
        Ok(a) => a,
        Err(e) => {
            remap_local(&mut local, source, origin, real_source);
            diagnostics.extend(&local);
            return Err(e);
        }
    };
    remap_local(&mut local, source, origin, real_source);
    diagnostics.extend(&local);
    let program = ast.get_root();
    let f = program.functions.first().ok_or_else(|| {
        Error::new(
            std::io::ErrorKind::InvalidData,
            "replace parse produced no function",
        )
    })?;
    for stmt in f.body {
        if let StatementNode::Return(Some(e)) = stmt {
            return Ok(e.clone());
        }
    }
    Err(Error::new(
        std::io::ErrorKind::InvalidData,
        "replace parse: expected return expression",
    ))
}

/// Rewrites `<syntax-replace>`-labeled diagnostics onto the origin block. Offsets inside
/// the generated source are preserved and clamped to the block; line/col are recomputed
/// against the user's real file so the rendered squiggle lands in their `html {}` block.
fn remap_local(
    local: &mut DiagnosticBag,
    source: &str,
    origin: Option<&GenOrigin>,
    real_source: Option<&str>,
) {
    let Some(origin) = origin else { return };
    let line_text = real_source.map(|src| LineText::new(src.to_string()));
    for d in local.diagnostics.iter_mut() {
        let Some(span) = d.span.take() else { continue };
        let off = span
            .start
            .saturating_sub(WRAP_PREFIX.len())
            .min(source.len());
        let abs = origin.block_start.saturating_add(off);
        d.file_path = Some(origin.real_file.clone());
        d.span = Some(match &line_text {
            Some(lt) => TextSpan::new((abs, abs + 1), lt),
            None => TextSpan {
                start: abs,
                end: abs + 1,
                line_no: 0,
                col_no: 0,
            },
        });
    }
}

/// Rebuilds a function body, replacing syntax blocks whose site key is in `by_site`.
pub fn rewrite_function_body<'a>(
    arena: &'a Bump,
    body: &'a [StatementNode<'a>],
    by_site: &IndexMap<(String, String), String>,
    diagnostics: &mut DiagnosticBag,
    file: Option<&str>,
    file_contents: &std::collections::HashMap<String, String>,
) -> Result<&'a [StatementNode<'a>], Error> {
    let mut changed = false;
    let mut out = Vec::with_capacity(body.len());
    for s in body {
        out.push(rewrite_stmt(arena, s, by_site, diagnostics, &mut changed, file, file_contents)?);
    }
    if changed {
        Ok(arena.alloc_slice_fill_iter(out))
    } else {
        Ok(body)
    }
}

pub fn rewrite_function<'a>(
    arena: &'a Bump,
    f: &mut FunctionNode<'a>,
    by_site: &IndexMap<(String, String), String>,
    diagnostics: &mut DiagnosticBag,
    file_contents: &std::collections::HashMap<String, String>,
) -> Result<(), Error> {
    let file = f.file_path.as_ref().map(|p| p.to_string());
    f.body = rewrite_function_body(
        arena,
        f.body,
        by_site,
        diagnostics,
        file.as_deref(),
        file_contents,
    )?;
    Ok(())
}

fn block_site_key(block: &dream_syntax::nodes::SyntaxBlockNode<'_>) -> (String, String) {
    let mut body_text = String::new();
    for part in &block.parts {
        match part {
            SyntaxBlockPart::Text(t) => body_text.push_str(t),
            SyntaxBlockPart::Splice(e) => {
                body_text.push('{');
                body_text.push_str(&super::syntax::expr_source_approx_pub(e));
                body_text.push('}');
            }
        }
    }
    (block.name.text.clone(), body_text)
}

#[allow(clippy::too_many_arguments)]
fn rewrite_expr<'a>(
    arena: &'a Bump,
    expr: &ExpressionNode<'a>,
    by_site: &IndexMap<(String, String), String>,
    diagnostics: &mut DiagnosticBag,
    changed: &mut bool,
    file: Option<&str>,
    file_contents: &std::collections::HashMap<String, String>,
) -> Result<ExpressionNode<'a>, Error> {
    if let ExpressionNode::SyntaxBlock(block) = expr {
        let key = block_site_key(block);
        if let Some(src) = by_site.get(&key) {
            *changed = true;
            // The generator's output stands in for this block; map diagnostics back to
            // the block's `{` in the user's file (offset within the output preserved).
            let origin = file.map(|f| GenOrigin {
                real_file: f.to_string(),
                block_start: block.block_span.start.saturating_add(1),
            });
            let real_source = match (&origin, file) {
                (Some(_), Some(f)) => file_contents.get(f).map(|s| s.as_str()),
                _ => None,
            };
            return parse_expression_source(
                arena,
                src,
                diagnostics,
                origin.as_ref(),
                real_source,
            );
        }
    }
    Ok(match expr {
        ExpressionNode::Binary(l, op, r) => ExpressionNode::Binary(
            arena.alloc(rewrite_expr(arena, l, by_site, diagnostics, changed, file, file_contents)?),
            op.clone(),
            arena.alloc(rewrite_expr(arena, r, by_site, diagnostics, changed, file, file_contents)?),
        ),
        ExpressionNode::Ternary(c, t, e) => ExpressionNode::Ternary(
            arena.alloc(rewrite_expr(arena, c, by_site, diagnostics, changed, file, file_contents)?),
            arena.alloc(rewrite_expr(arena, t, by_site, diagnostics, changed, file, file_contents)?),
            arena.alloc(rewrite_expr(arena, e, by_site, diagnostics, changed, file, file_contents)?),
        ),
        ExpressionNode::Unary(op, x) => ExpressionNode::Unary(
            op.clone(),
            arena.alloc(rewrite_expr(arena, x, by_site, diagnostics, changed, file, file_contents)?),
        ),
        ExpressionNode::IncDec {
            prefix,
            is_inc,
            target,
            op,
        } => ExpressionNode::IncDec {
            prefix: *prefix,
            is_inc: *is_inc,
            target: arena.alloc(rewrite_expr(arena, target, by_site, diagnostics, changed, file, file_contents)?),
            op: op.clone(),
        },
        ExpressionNode::Parenthesized(open, x) => ExpressionNode::Parenthesized(
            open.clone(),
            arena.alloc(rewrite_expr(arena, x, by_site, diagnostics, changed, file, file_contents)?),
        ),
        ExpressionNode::Await(await_tok, x) => ExpressionNode::Await(
            await_tok.clone(),
            arena.alloc(rewrite_expr(arena, x, by_site, diagnostics, changed, file, file_contents)?),
        ),
        ExpressionNode::Try(x) => {
            ExpressionNode::Try(arena.alloc(rewrite_expr(arena, x, by_site, diagnostics, changed, file, file_contents)?))
        }
        ExpressionNode::Cast(open, ty, x) => ExpressionNode::Cast(
            open.clone(),
            ty.clone(),
            arena.alloc(rewrite_expr(arena, x, by_site, diagnostics, changed, file, file_contents)?),
        ),
        ExpressionNode::IsExpression(x, ty, b) => ExpressionNode::IsExpression(
            arena.alloc(rewrite_expr(arena, x, by_site, diagnostics, changed, file, file_contents)?),
            ty.clone(),
            b.clone(),
        ),
        ExpressionNode::IndexAccess(a, i) => ExpressionNode::IndexAccess(
            arena.alloc(rewrite_expr(arena, a, by_site, diagnostics, changed, file, file_contents)?),
            arena.alloc(rewrite_expr(arena, i, by_site, diagnostics, changed, file, file_contents)?),
        ),
        ExpressionNode::MemberAccess(r, m) => ExpressionNode::MemberAccess(
            arena.alloc(rewrite_expr(arena, r, by_site, diagnostics, changed, file, file_contents)?),
            m.clone(),
        ),
        ExpressionNode::RefArgument(ref_tok, x) => ExpressionNode::RefArgument(
            ref_tok.clone(),
            arena.alloc(rewrite_expr(arena, x, by_site, diagnostics, changed, file, file_contents)?),
        ),
        ExpressionNode::NamedArg(n, x) => ExpressionNode::NamedArg(
            n.clone(),
            arena.alloc(rewrite_expr(arena, x, by_site, diagnostics, changed, file, file_contents)?),
        ),
        ExpressionNode::Call(c, gens, args) => {
            let mut nargs = Vec::new();
            for a in args {
                nargs.push(rewrite_expr(arena, a, by_site, diagnostics, changed, file, file_contents)?);
            }
            ExpressionNode::Call(
                arena.alloc(rewrite_expr(arena, c, by_site, diagnostics, changed, file, file_contents)?),
                gens.clone(),
                nargs,
            )
        }
        ExpressionNode::MethodCall(r, name, gens, args) => {
            let mut nargs = Vec::new();
            for a in args {
                nargs.push(rewrite_expr(arena, a, by_site, diagnostics, changed, file, file_contents)?);
            }
            ExpressionNode::MethodCall(
                arena.alloc(rewrite_expr(arena, r, by_site, diagnostics, changed, file, file_contents)?),
                name.clone(),
                gens.clone(),
                nargs,
            )
        }
        ExpressionNode::FunctionCall(name, gens, args) => {
            let mut nargs = Vec::new();
            for a in args {
                nargs.push(rewrite_expr(arena, a, by_site, diagnostics, changed, file, file_contents)?);
            }
            ExpressionNode::FunctionCall(name.clone(), gens.clone(), nargs)
        }
        ExpressionNode::ArrayLiteral(open, args) => {
            let mut nargs = Vec::new();
            for a in args {
                nargs.push(rewrite_expr(arena, a, by_site, diagnostics, changed, file, file_contents)?);
            }
            ExpressionNode::ArrayLiteral(open.clone(), nargs)
        }
        ExpressionNode::TupleLiteral(open, args) => {
            let mut nargs = Vec::new();
            for a in args {
                nargs.push(rewrite_expr(arena, a, by_site, diagnostics, changed, file, file_contents)?);
            }
            ExpressionNode::TupleLiteral(open.clone(), nargs)
        }
        ExpressionNode::SetLiteral(open, args) => {
            let mut nargs = Vec::new();
            for a in args {
                nargs.push(rewrite_expr(arena, a, by_site, diagnostics, changed, file, file_contents)?);
            }
            ExpressionNode::SetLiteral(open.clone(), nargs)
        }
        ExpressionNode::MapLiteral(open, entries) => {
            let mut nentries = Vec::new();
            for (k, v) in entries {
                nentries.push((
                    rewrite_expr(arena, k, by_site, diagnostics, changed, file, file_contents)?,
                    rewrite_expr(arena, v, by_site, diagnostics, changed, file, file_contents)?,
                ));
            }
            ExpressionNode::MapLiteral(open.clone(), nentries)
        }
        ExpressionNode::Switch(switch_tok, subj, arms) => {
            let nsubj = arena.alloc(rewrite_expr(arena, subj, by_site, diagnostics, changed, file, file_contents)?);
            let mut narms = Vec::new();
            for arm in arms {
                let guard = match &arm.guard {
                    Some(g) => Some(rewrite_expr(arena, g, by_site, diagnostics, changed, file, file_contents)?),
                    None => None,
                };
                let body = match &arm.body {
                    SwitchArmBody::Expr(e) => {
                        SwitchArmBody::Expr(rewrite_expr(arena, e, by_site, diagnostics, changed, file, file_contents)?)
                    }
                    SwitchArmBody::Block(stmts) => {
                        let nb = rewrite_function_body(arena, stmts, by_site, diagnostics, file, file_contents)?;
                        SwitchArmBody::Block(nb)
                    }
                };
                narms.push(SwitchArm {
                    pattern: arm.pattern.clone(),
                    guard,
                    body,
                });
            }
            ExpressionNode::Switch(switch_tok.clone(), nsubj, narms)
        }
        ExpressionNode::Lambda(l) => {
            let body = match &l.body {
                LambdaBody::Expr(e) => LambdaBody::Expr(arena.alloc(rewrite_expr(
                    arena,
                    e,
                    by_site,
                    diagnostics,
                    changed,
                    file, file_contents,
                )?)),
                LambdaBody::Block(stmts) => {
                    LambdaBody::Block(rewrite_function_body(arena, stmts, by_site, diagnostics, file, file_contents)?)
                }
            };
            let mut nl = (*l).clone();
            nl.body = body;
            ExpressionNode::Lambda(arena.alloc(nl))
        }
        ExpressionNode::Literal(_)
        | ExpressionNode::Identifier(_)
        | ExpressionNode::SizeOf(_, _)
        | ExpressionNode::NameOf(_, _)
        | ExpressionNode::SyntaxBlock(_) => expr.clone(),
    })
}

#[allow(clippy::too_many_arguments)]
fn rewrite_stmt<'a>(
    arena: &'a Bump,
    stmt: &StatementNode<'a>,
    by_site: &IndexMap<(String, String), String>,
    diagnostics: &mut DiagnosticBag,
    changed: &mut bool,
    file: Option<&str>,
    file_contents: &std::collections::HashMap<String, String>,
) -> Result<StatementNode<'a>, Error> {
    Ok(match stmt {
        StatementNode::ExpressionStatement(e) => StatementNode::ExpressionStatement(rewrite_expr(
            arena,
            e,
            by_site,
            diagnostics,
            changed,
                file, file_contents,
        )?),
        StatementNode::AwaitStmt(e) => {
            StatementNode::AwaitStmt(rewrite_expr(arena, e, by_site, diagnostics, changed, file, file_contents)?)
        }
        StatementNode::Return(Some(e)) => {
            StatementNode::Return(Some(rewrite_expr(arena, e, by_site, diagnostics, changed, file, file_contents)?))
        }
        StatementNode::Return(None) => StatementNode::Return(None),
        StatementNode::Assignment(n, e) => StatementNode::Assignment(
            n.clone(),
            rewrite_expr(arena, e, by_site, diagnostics, changed, file, file_contents)?,
        ),
        StatementNode::Declaration(n, ty, e, c) => StatementNode::Declaration(
            n.clone(),
            ty.clone(),
            rewrite_expr(arena, e, by_site, diagnostics, changed, file, file_contents)?,
            *c,
        ),
        StatementNode::TupleDeclaration {
            pattern,
            ty,
            init,
            is_const,
        } => StatementNode::TupleDeclaration {
            pattern: pattern.clone(),
            ty: ty.clone(),
            init: rewrite_expr(arena, init, by_site, diagnostics, changed, file, file_contents)?,
            is_const: *is_const,
        },
        StatementNode::IndexAssignment(a, i, v) => StatementNode::IndexAssignment(
            arena.alloc(rewrite_expr(arena, a, by_site, diagnostics, changed, file, file_contents)?),
            arena.alloc(rewrite_expr(arena, i, by_site, diagnostics, changed, file, file_contents)?),
            rewrite_expr(arena, v, by_site, diagnostics, changed, file, file_contents)?,
        ),
        StatementNode::MemberAssignment(r, m, v) => StatementNode::MemberAssignment(
            arena.alloc(rewrite_expr(arena, r, by_site, diagnostics, changed, file, file_contents)?),
            m.clone(),
            rewrite_expr(arena, v, by_site, diagnostics, changed, file, file_contents)?,
        ),
        StatementNode::FunctionInvocation(n, g, args) => {
            let mut nargs = Vec::new();
            for a in args {
                nargs.push(rewrite_expr(arena, a, by_site, diagnostics, changed, file, file_contents)?);
            }
            StatementNode::FunctionInvocation(n.clone(), g.clone(), nargs)
        }
        StatementNode::MethodInvocation(r, n, g, args) => {
            let mut nargs = Vec::new();
            for a in args {
                nargs.push(rewrite_expr(arena, a, by_site, diagnostics, changed, file, file_contents)?);
            }
            StatementNode::MethodInvocation(
                arena.alloc(rewrite_expr(arena, r, by_site, diagnostics, changed, file, file_contents)?),
                n.clone(),
                g.clone(),
                nargs,
            )
        }
        StatementNode::IfElse(cond, then_b, elifs, else_b) => {
            let ncond = rewrite_expr(arena, cond, by_site, diagnostics, changed, file, file_contents)?;
            let nthen = rewrite_function_body(arena, then_b, by_site, diagnostics, file, file_contents)?;
            let mut nelifs = Vec::new();
            for (c, b) in elifs {
                nelifs.push((
                    rewrite_expr(arena, c, by_site, diagnostics, changed, file, file_contents)?,
                    rewrite_function_body(arena, b, by_site, diagnostics, file, file_contents)?,
                ));
            }
            let nelse = match else_b {
                Some(b) => Some(rewrite_function_body(arena, b, by_site, diagnostics, file, file_contents)?),
                None => None,
            };
            StatementNode::IfElse(ncond, nthen, nelifs, nelse)
        }
        StatementNode::While(cond, body) => StatementNode::While(
            rewrite_expr(arena, cond, by_site, diagnostics, changed, file, file_contents)?,
            rewrite_function_body(arena, body, by_site, diagnostics, file, file_contents)?,
        ),
        StatementNode::Lock(cond, body) => StatementNode::Lock(
            rewrite_expr(arena, cond, by_site, diagnostics, changed, file, file_contents)?,
            rewrite_function_body(arena, body, by_site, diagnostics, file, file_contents)?,
        ),
        StatementNode::Defer(budget, body) => StatementNode::Defer(
            match budget {
                Some(q) => Some(rewrite_expr(arena, q, by_site, diagnostics, changed, file, file_contents)?),
                None => None,
            },
            rewrite_function_body(arena, body, by_site, diagnostics, file, file_contents)?,
        ),
        StatementNode::DoWhile(body, cond) => StatementNode::DoWhile(
            rewrite_function_body(arena, body, by_site, diagnostics, file, file_contents)?,
            rewrite_expr(arena, cond, by_site, diagnostics, changed, file, file_contents)?,
        ),
        StatementNode::For(init, cond, inc, body) => {
            let ninit = match init {
                Some(s) => {
                    Some(&*arena.alloc(rewrite_stmt(arena, s, by_site, diagnostics, changed, file, file_contents)?))
                }
                None => None,
            };
            let ncond = match cond {
                Some(e) => Some(rewrite_expr(arena, e, by_site, diagnostics, changed, file, file_contents)?),
                None => None,
            };
            let ninc = match inc {
                Some(s) => {
                    Some(&*arena.alloc(rewrite_stmt(arena, s, by_site, diagnostics, changed, file, file_contents)?))
                }
                None => None,
            };
            StatementNode::For(
                ninit,
                ncond,
                ninc,
                rewrite_function_body(arena, body, by_site, diagnostics, file, file_contents)?,
            )
        }
        StatementNode::ForEach(n, iter, a, b, body) => StatementNode::ForEach(
            n.clone(),
            rewrite_expr(arena, iter, by_site, diagnostics, changed, file, file_contents)?,
            a.clone(),
            b.clone(),
            rewrite_function_body(arena, body, by_site, diagnostics, file, file_contents)?,
        ),
        StatementNode::Switch(subj, cases, default) => {
            let nsubj = rewrite_expr(arena, subj, by_site, diagnostics, changed, file, file_contents)?;
            let mut ncases = Vec::new();
            for (labels, body) in cases {
                let mut nlabels = Vec::new();
                for l in labels {
                    nlabels.push(rewrite_expr(arena, l, by_site, diagnostics, changed, file, file_contents)?);
                }
                ncases.push((
                    nlabels,
                    rewrite_function_body(arena, body, by_site, diagnostics, file, file_contents)?,
                ));
            }
            let ndef = match default {
                Some(b) => Some(rewrite_function_body(arena, b, by_site, diagnostics, file, file_contents)?),
                None => None,
            };
            StatementNode::Switch(nsubj, ncases, ndef)
        }
        StatementNode::Labeled(l, inner) => StatementNode::Labeled(
            l.clone(),
            arena.alloc(rewrite_stmt(arena, inner, by_site, diagnostics, changed, file, file_contents)?),
        ),
        StatementNode::Break(x) => StatementNode::Break(x.clone()),
        StatementNode::Continue(x) => StatementNode::Continue(x.clone()),
        StatementNode::WorkgroupDecl(n, ty, size) => {
            StatementNode::WorkgroupDecl(n.clone(), ty.clone(), *size)
        }
    })
}
