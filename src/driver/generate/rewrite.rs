//! In-arena rewrite of `SyntaxBlock` expressions into ordinary Dream expressions.

use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;
use dream_text::line_text::LineText;
use dream_text::text_span::TextSpan;
use dream_syntax::lexer::Lexer;
use dream_syntax::nodes::function::FunctionNode;
use dream_syntax::nodes::{
    ExpressionNode, LambdaBody, LambdaNode, PatternNode, StatementNode, SwitchArm, SwitchArmBody,
    SyntaxBlockNode, SyntaxBlockPart,
};
use dream_syntax::token::syntax_token::SyntaxToken;
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
            let mut replaced =
                parse_expression_source(arena, src, diagnostics, origin.as_ref(), real_source)?;
            if let Some(o) = origin {
                // Analyzer diagnostics attribute file from the enclosing function and spans
                // from these nodes — shift wrapper-relative positions into the block region
                // so errors render inside the user's html {} block.
                let line_text = real_source.map(|src| LineText::new(src.to_string()));
                if let Some(lt) = line_text {
                    let map = SpanMap {
                        delta: (o.block_start as isize + 1) - (WRAP_PREFIX.len() as isize),
                        lo: o.block_start,
                        hi: o.block_start + src.len(),
                        line_text: lt,
                    };
                    replaced = shift_expr(&map, arena, &replaced);
                }
            }
            return Ok(replaced);
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
        ExpressionNode::ArrayRepeat(open, v, n) => ExpressionNode::ArrayRepeat(
            open.clone(),
            Box::new(rewrite_expr(arena, v, by_site, diagnostics, changed, file, file_contents)?),
            Box::new(rewrite_expr(arena, n, by_site, diagnostics, changed, file, file_contents)?),
        ),
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

/// Maps wrapper-relative token positions onto the origin block in the user's file: each
/// point shifts by `delta` and clamps to `[lo, hi]`; line/col are recomputed against the
/// user's real source so rendered excerpts land inside their `html {}` block.
struct SpanMap {
    delta: isize,
    lo: usize,
    hi: usize,
    line_text: LineText,
}

impl SpanMap {
    fn map_point(&self, p: usize) -> usize {
        let shifted = (p as isize + self.delta).max(0) as usize;
        shifted.clamp(self.lo, self.hi)
    }

    fn token(&self, t: &SyntaxToken) -> SyntaxToken {
        let mut t = t.clone();
        let start = self.map_point(t.position.start);
        let end = self.map_point(t.position.end);
        let (line_no, col_no) = self.line_text.get_point(start);
        t.position = TextSpan {
            start,
            end,
            line_no,
            col_no,
        };
        t
    }

    fn span(&self, s: TextSpan) -> TextSpan {
        let start = self.map_point(s.start);
        let (line_no, col_no) = self.line_text.get_point(start);
        TextSpan {
            start,
            end: self.map_point(s.end),
            line_no,
            col_no,
        }
    }
}



/// Rebuilds `e` with every token position mapped through `map`. Mirrors the variant
/// enumeration in [`rewrite_expr`]; `Type` payloads are passed through unshifted (their
/// positions rarely anchor diagnostics).
fn shift_expr<'a>(map: &SpanMap, arena: &'a Bump, e: &ExpressionNode<'a>) -> ExpressionNode<'a> {
    match e {
        ExpressionNode::Literal(_) => e.clone(),
        ExpressionNode::ArrayLiteral(t, xs) => ExpressionNode::ArrayLiteral(
            map.token(t),
            xs.iter().map(|x| shift_expr(map, arena, x)).collect(),
        ),
        ExpressionNode::ArrayRepeat(t, v, n) => ExpressionNode::ArrayRepeat(
            map.token(t),
            Box::new(shift_expr(map, arena, v)),
            Box::new(shift_expr(map, arena, n)),
        ),
        ExpressionNode::TupleLiteral(t, xs) => ExpressionNode::TupleLiteral(
            map.token(t),
            xs.iter().map(|x| shift_expr(map, arena, x)).collect(),
        ),
        ExpressionNode::SetLiteral(t, xs) => ExpressionNode::SetLiteral(
            map.token(t),
            xs.iter().map(|x| shift_expr(map, arena, x)).collect(),
        ),
        ExpressionNode::MapLiteral(t, kvs) => ExpressionNode::MapLiteral(
            map.token(t),
            kvs.iter()
                .map(|(k, v)| (shift_expr(map, arena, k), shift_expr(map, arena, v)))
                .collect(),
        ),
        ExpressionNode::Binary(l, t, r) => ExpressionNode::Binary(
            arena.alloc(shift_expr(map, arena, l)),
            map.token(t),
            arena.alloc(shift_expr(map, arena, r)),
        ),
        ExpressionNode::Unary(t, x) => {
            ExpressionNode::Unary(map.token(t), arena.alloc(shift_expr(map, arena, x)))
        }
        ExpressionNode::IncDec {
            prefix,
            is_inc,
            target,
            op,
        } => ExpressionNode::IncDec {
            prefix: *prefix,
            is_inc: *is_inc,
            target: arena.alloc(shift_expr(map, arena, target)),
            op: map.token(op),
        },
        ExpressionNode::Identifier(t) => ExpressionNode::Identifier(map.token(t)),
        ExpressionNode::Parenthesized(t, x) => {
            ExpressionNode::Parenthesized(map.token(t), arena.alloc(shift_expr(map, arena, x)))
        }
        ExpressionNode::FunctionCall(t, tys, args) => ExpressionNode::FunctionCall(
            map.token(t),
            tys.clone(),
            args.iter().map(|a| shift_expr(map, arena, a)).collect(),
        ),
        ExpressionNode::Call(callee, tys, args) => ExpressionNode::Call(
            arena.alloc(shift_expr(map, arena, callee)),
            tys.clone(),
            args.iter().map(|a| shift_expr(map, arena, a)).collect(),
        ),
        ExpressionNode::IndexAccess(a, i) => ExpressionNode::IndexAccess(
            arena.alloc(shift_expr(map, arena, a)),
            arena.alloc(shift_expr(map, arena, i)),
        ),
        ExpressionNode::Cast(t, ty, x) => {
            ExpressionNode::Cast(map.token(t), ty.clone(), arena.alloc(shift_expr(map, arena, x)))
        }
        ExpressionNode::SizeOf(t, ty) => ExpressionNode::SizeOf(map.token(t), ty.clone()),
        ExpressionNode::NameOf(t, path) => {
            ExpressionNode::NameOf(map.token(t), path.iter().map(|p| map.token(p)).collect())
        }
        ExpressionNode::MemberAccess(x, t) => ExpressionNode::MemberAccess(
            arena.alloc(shift_expr(map, arena, x)),
            map.token(t),
        ),
        ExpressionNode::IsExpression(x, ty, bind) => ExpressionNode::IsExpression(
            arena.alloc(shift_expr(map, arena, x)),
            ty.clone(),
            bind.as_ref().map(|t| map.token(t)),
        ),
        ExpressionNode::MethodCall(x, t, tys, args) => ExpressionNode::MethodCall(
            arena.alloc(shift_expr(map, arena, x)),
            map.token(t),
            tys.clone(),
            args.iter().map(|a| shift_expr(map, arena, a)).collect(),
        ),
        ExpressionNode::Ternary(a, b, c) => ExpressionNode::Ternary(
            arena.alloc(shift_expr(map, arena, a)),
            arena.alloc(shift_expr(map, arena, b)),
            arena.alloc(shift_expr(map, arena, c)),
        ),
        ExpressionNode::Await(t, x) => {
            ExpressionNode::Await(map.token(t), arena.alloc(shift_expr(map, arena, x)))
        }
        ExpressionNode::Switch(t, subject, arms) => {
            let subject = arena.alloc(shift_expr(map, arena, subject));
            let arms_v: Vec<SwitchArm> = arms
                .iter()
                .map(|arm| shift_arm(map, arena, arm))
                .collect();
            ExpressionNode::Switch(map.token(t), subject, arms_v)
        }
        ExpressionNode::Try(x) => ExpressionNode::Try(arena.alloc(shift_expr(map, arena, x))),
        ExpressionNode::Lambda(l) => ExpressionNode::Lambda(arena.alloc(shift_lambda(map, arena, l))),
        ExpressionNode::NamedArg(t, x) => {
            ExpressionNode::NamedArg(map.token(t), arena.alloc(shift_expr(map, arena, x)))
        }
        ExpressionNode::RefArgument(t, x) => {
            ExpressionNode::RefArgument(map.token(t), arena.alloc(shift_expr(map, arena, x)))
        }
        ExpressionNode::SyntaxBlock(n) => {
            let parts: Vec<SyntaxBlockPart> = n
                .parts
                .iter()
                .map(|part| match part {
                    SyntaxBlockPart::Text(t) => SyntaxBlockPart::Text(t.clone()),
                    SyntaxBlockPart::Splice(x) => {
                        SyntaxBlockPart::Splice(arena.alloc(shift_expr(map, arena, x)))
                    }
                })
                .collect();
            ExpressionNode::SyntaxBlock(arena.alloc(SyntaxBlockNode {
                name: map.token(&n.name),
                block_span: map.span(n.block_span),
                parts,
            }))
        }
    }
}

fn shift_arm<'a>(map: &SpanMap, arena: &'a Bump, arm: &SwitchArm<'a>) -> SwitchArm<'a> {
    SwitchArm {
        pattern: shift_pattern(map, &arm.pattern),
        guard: arm.guard.as_ref().map(|g| shift_expr(map, arena, g)),
        body: match &arm.body {
            SwitchArmBody::Expr(e) => SwitchArmBody::Expr(shift_expr(map, arena, e)),
            SwitchArmBody::Block(b) => SwitchArmBody::Block(shift_stmts(map, arena, b)),
        },
    }
}

fn shift_lambda<'a>(map: &SpanMap, arena: &'a Bump, l: &LambdaNode<'a>) -> LambdaNode<'a> {
    LambdaNode {
        open_paren_position: map.span(l.open_paren_position),
        async_keyword: l.async_keyword.as_ref().map(|s| map.span(*s)),
        is_async: l.is_async,
        generic_parameters: l
            .generic_parameters
            .as_ref()
            .map(|ts| ts.iter().map(|t| map.token(t)).collect()),
        generic_constraints: l.generic_constraints.clone(),
        parameters: l.parameters.clone(),
        body: match &l.body {
            LambdaBody::Expr(e) => LambdaBody::Expr(arena.alloc(shift_expr(map, arena, e))),
            LambdaBody::Block(b) => LambdaBody::Block(shift_stmts(map, arena, b)),
        },
    }
}

fn shift_pattern(map: &SpanMap, p: &PatternNode) -> PatternNode {
    match p {
        PatternNode::Wildcard(t) => PatternNode::Wildcard(map.token(t)),
        PatternNode::Binding(t) => PatternNode::Binding(map.token(t)),
        PatternNode::Literal(_) => p.clone(),
        PatternNode::Variant(q, name, sub) => PatternNode::Variant(
            q.as_ref().map(|t| map.token(t)),
            map.token(name),
            sub.iter().map(|sp| shift_pattern(map, sp)).collect(),
        ),
        PatternNode::Range(_, _) => p.clone(),
        PatternNode::Or(alts) => {
            PatternNode::Or(alts.iter().map(|sp| shift_pattern(map, sp)).collect())
        }
        PatternNode::Tuple(sub) => {
            PatternNode::Tuple(sub.iter().map(|sp| shift_pattern(map, sp)).collect())
        }
    }
}

fn shift_stmts<'a>(
    map: &SpanMap,
    arena: &'a Bump,
    stmts: &'a [StatementNode<'a>],
) -> &'a [StatementNode<'a>] {
    stmts
        .iter()
        .map(|st| shift_stmt(map, arena, st))
        .collect::<Vec<_>>()
        .leak()
}

fn shift_stmt<'a>(map: &SpanMap, arena: &'a Bump, st: &StatementNode<'a>) -> StatementNode<'a> {
    match st {
        StatementNode::Assignment(t, e) => {
            StatementNode::Assignment(map.token(t), shift_expr(map, arena, e))
        }
        StatementNode::IndexAssignment(a, i, v) => StatementNode::IndexAssignment(
            arena.alloc(shift_expr(map, arena, a)),
            arena.alloc(shift_expr(map, arena, i)),
            shift_expr(map, arena, v),
        ),
        StatementNode::MemberAssignment(x, t, v) => StatementNode::MemberAssignment(
            arena.alloc(shift_expr(map, arena, x)),
            map.token(t),
            shift_expr(map, arena, v),
        ),
        StatementNode::Declaration(t, ty, e, c) => StatementNode::Declaration(
            map.token(t),
            ty.clone(),
            shift_expr(map, arena, e),
            *c,
        ),
        StatementNode::TupleDeclaration {
            pattern,
            ty,
            init,
            is_const,
        } => StatementNode::TupleDeclaration {
            pattern: shift_pattern(map, pattern),
            ty: ty.clone(),
            init: shift_expr(map, arena, init),
            is_const: *is_const,
        },
        StatementNode::FunctionInvocation(t, tys, args) => StatementNode::FunctionInvocation(
            map.token(t),
            tys.clone(),
            args.iter().map(|a| shift_expr(map, arena, a)).collect(),
        ),
        StatementNode::MethodInvocation(x, t, tys, args) => StatementNode::MethodInvocation(
            arena.alloc(shift_expr(map, arena, x)),
            map.token(t),
            tys.clone(),
            args.iter().map(|a| shift_expr(map, arena, a)).collect(),
        ),
        StatementNode::Return(e) => {
            StatementNode::Return(e.as_ref().map(|x| shift_expr(map, arena, x)))
        }
        StatementNode::IfElse(cond, then_b, elifs, else_b) => StatementNode::IfElse(
            shift_expr(map, arena, cond),
            shift_stmts(map, arena, then_b),
            {
                let collected: Vec<(ExpressionNode, &[StatementNode])> = elifs
                    .iter()
                    .map(|(c, b)| (shift_expr(map, arena, c), shift_stmts(map, arena, b)))
                    .collect();
                collected
            },
            else_b.map(|b| shift_stmts(map, arena, b)),
        ),
        StatementNode::While(c, body) => {
            StatementNode::While(shift_expr(map, arena, c), shift_stmts(map, arena, body))
        }
        StatementNode::DoWhile(body, c) => {
            StatementNode::DoWhile(shift_stmts(map, arena, body), shift_expr(map, arena, c))
        }
        StatementNode::For(init, cond, step, body) => StatementNode::For(
            init.as_ref()
                .map(|st| &*arena.alloc(shift_stmt(map, arena, st))),
            cond.as_ref().map(|c| shift_expr(map, arena, c)),
            step.as_ref()
                .map(|st| &*arena.alloc(shift_stmt(map, arena, st))),
            shift_stmts(map, arena, body),
        ),
        StatementNode::Labeled(label, inner) => {
            StatementNode::Labeled(label.clone(), arena.alloc(shift_stmt(map, arena, inner)))
        }
        other @ (StatementNode::Break(_) | StatementNode::Continue(_)) => other.clone(),
        StatementNode::ExpressionStatement(e) => {
            StatementNode::ExpressionStatement(shift_expr(map, arena, e))
        }
        StatementNode::AwaitStmt(e) => StatementNode::AwaitStmt(shift_expr(map, arena, e)),
        StatementNode::ForEach(t, iter, idx, tmp, body) => StatementNode::ForEach(
            map.token(t),
            shift_expr(map, arena, iter),
            idx.clone(),
            tmp.clone(),
            shift_stmts(map, arena, body),
        ),
        StatementNode::Switch(subject, cases, default_b) => StatementNode::Switch(
            shift_expr(map, arena, subject),
            {
                let collected: Vec<(Vec<ExpressionNode>, &[StatementNode])> = cases
                    .iter()
                    .map(|(labels, body)| {
                        (
                            labels
                                .iter()
                                .map(|l| shift_expr(map, arena, l))
                                .collect(),
                            shift_stmts(map, arena, body),
                        )
                    })
                    .collect();
                collected
            },
            default_b.map(|b| shift_stmts(map, arena, b)),
        ),
        StatementNode::Lock(target, body) => {
            StatementNode::Lock(shift_expr(map, arena, target), shift_stmts(map, arena, body))
        }
        StatementNode::Defer(q, body) => StatementNode::Defer(
            q.as_ref().map(|x| shift_expr(map, arena, x)),
            shift_stmts(map, arena, body),
        ),
        StatementNode::WorkgroupDecl(t, ty, n) => {
            StatementNode::WorkgroupDecl(map.token(t), ty.clone(), *n)
        }
    }
}
