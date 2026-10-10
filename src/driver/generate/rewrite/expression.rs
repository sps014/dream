use super::functions::rewrite_function_body;
use super::model::GenOrigin;
use super::model::Rewriter;
use super::model::WRAP_PREFIX;
use super::parse::parse_expression_source;
use super::spans::SpanMap;
use super::spans::shift_expr;
use dream_syntax::nodes::ExpressionNode;
use dream_syntax::nodes::LambdaBody;
use dream_syntax::nodes::SwitchArm;
use dream_syntax::nodes::SwitchArmBody;
use dream_text::line_text::LineText;
use std::io::Error;

impl<'a> Rewriter<'a, '_> {
    pub(super) fn expression(
        &mut self,
        expr: &ExpressionNode<'a>,
    ) -> Result<ExpressionNode<'a>, Error> {
        let arena = self.arena;
        if let ExpressionNode::SyntaxBlock(block) = expr {
            let key = super::super::sites::site_key(self.file, block);
            if let Some(src) = self.by_site.get(&key) {
                *self.changed = true;
                // The generator's output stands in for this block; map diagnostics back to
                // the block's `{` in the user's file (offset within the output preserved).
                let origin = self.file.map(|f| GenOrigin {
                    real_file: f.to_string(),
                    block_start: block.block_span.start.saturating_add(1),
                });
                let real_source = match (&origin, self.file) {
                    (Some(_), Some(f)) => self.file_contents.get(f).map(|s| s.as_str()),
                    _ => None,
                };
                let mut replaced = parse_expression_source(
                    arena,
                    src,
                    self.diagnostics,
                    origin.as_ref(),
                    real_source,
                )?;
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
                arena.alloc(self.expression(l)?),
                op.clone(),
                arena.alloc(self.expression(r)?),
            ),
            ExpressionNode::Ternary(c, t, e) => ExpressionNode::Ternary(
                arena.alloc(self.expression(c)?),
                arena.alloc(self.expression(t)?),
                arena.alloc(self.expression(e)?),
            ),
            ExpressionNode::Unary(op, x) => {
                ExpressionNode::Unary(op.clone(), arena.alloc(self.expression(x)?))
            }
            ExpressionNode::IncDec {
                prefix,
                is_inc,
                target,
                op,
            } => ExpressionNode::IncDec {
                prefix: *prefix,
                is_inc: *is_inc,
                target: arena.alloc(self.expression(target)?),
                op: op.clone(),
            },
            ExpressionNode::Parenthesized(open, x) => {
                ExpressionNode::Parenthesized(open.clone(), arena.alloc(self.expression(x)?))
            }
            ExpressionNode::Await(await_tok, x) => {
                ExpressionNode::Await(await_tok.clone(), arena.alloc(self.expression(x)?))
            }
            ExpressionNode::Try(x) => ExpressionNode::Try(arena.alloc(self.expression(x)?)),
            ExpressionNode::Cast(open, ty, x) => {
                ExpressionNode::Cast(open.clone(), ty.clone(), arena.alloc(self.expression(x)?))
            }
            ExpressionNode::IsExpression(x, ty, b) => ExpressionNode::IsExpression(
                arena.alloc(self.expression(x)?),
                ty.clone(),
                b.clone(),
            ),
            ExpressionNode::TypeOf(kw, x) => {
                ExpressionNode::TypeOf(kw.clone(), arena.alloc(self.expression(x)?))
            }
            ExpressionNode::IndexAccess(a, i) => ExpressionNode::IndexAccess(
                arena.alloc(self.expression(a)?),
                arena.alloc(self.expression(i)?),
            ),
            ExpressionNode::MemberAccess(r, m) => {
                ExpressionNode::MemberAccess(arena.alloc(self.expression(r)?), m.clone())
            }
            ExpressionNode::RefArgument(ref_tok, x) => {
                ExpressionNode::RefArgument(ref_tok.clone(), arena.alloc(self.expression(x)?))
            }
            ExpressionNode::NamedArg(n, x) => {
                ExpressionNode::NamedArg(n.clone(), arena.alloc(self.expression(x)?))
            }
            ExpressionNode::Call(c, gens, args) => {
                let mut nargs = Vec::new();
                for a in args {
                    nargs.push(self.expression(a)?);
                }
                ExpressionNode::Call(arena.alloc(self.expression(c)?), gens.clone(), nargs)
            }
            ExpressionNode::MethodCall(r, name, gens, args) => {
                let mut nargs = Vec::new();
                for a in args {
                    nargs.push(self.expression(a)?);
                }
                ExpressionNode::MethodCall(
                    arena.alloc(self.expression(r)?),
                    name.clone(),
                    gens.clone(),
                    nargs,
                )
            }
            ExpressionNode::FunctionCall(name, gens, args) => {
                let mut nargs = Vec::new();
                for a in args {
                    nargs.push(self.expression(a)?);
                }
                ExpressionNode::FunctionCall(name.clone(), gens.clone(), nargs)
            }
            ExpressionNode::ArrayLiteral(open, args) => {
                let mut nargs = Vec::new();
                for a in args {
                    nargs.push(self.expression(a)?);
                }
                ExpressionNode::ArrayLiteral(open.clone(), nargs)
            }
            ExpressionNode::ArrayRepeat(open, v, n) => ExpressionNode::ArrayRepeat(
                open.clone(),
                Box::new(self.expression(v)?),
                Box::new(self.expression(n)?),
            ),
            ExpressionNode::TupleLiteral(open, args) => {
                let mut nargs = Vec::new();
                for a in args {
                    nargs.push(self.expression(a)?);
                }
                ExpressionNode::TupleLiteral(open.clone(), nargs)
            }
            ExpressionNode::SetLiteral(open, args) => {
                let mut nargs = Vec::new();
                for a in args {
                    nargs.push(self.expression(a)?);
                }
                ExpressionNode::SetLiteral(open.clone(), nargs)
            }
            ExpressionNode::MapLiteral(open, entries) => {
                let mut nentries = Vec::new();
                for (k, v) in entries {
                    nentries.push((self.expression(k)?, self.expression(v)?));
                }
                ExpressionNode::MapLiteral(open.clone(), nentries)
            }
            ExpressionNode::Switch(switch_tok, subj, arms) => {
                let nsubj = arena.alloc(self.expression(subj)?);
                let mut narms = Vec::new();
                for arm in arms {
                    let guard = match &arm.guard {
                        Some(g) => Some(self.expression(g)?),
                        None => None,
                    };
                    let body = match &arm.body {
                        SwitchArmBody::Expr(e) => SwitchArmBody::Expr(self.expression(e)?),
                        SwitchArmBody::Block(stmts) => {
                            let nb = rewrite_function_body(
                                arena,
                                stmts,
                                self.by_site,
                                self.diagnostics,
                                self.file,
                                self.file_contents,
                            )?;
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
                    LambdaBody::Expr(e) => LambdaBody::Expr(arena.alloc(self.expression(e)?)),
                    LambdaBody::Block(stmts) => LambdaBody::Block(rewrite_function_body(
                        arena,
                        stmts,
                        self.by_site,
                        self.diagnostics,
                        self.file,
                        self.file_contents,
                    )?),
                };
                let mut nl = (*l).clone();
                nl.body = body;
                ExpressionNode::Lambda(arena.alloc(nl))
            }
            ExpressionNode::Literal(_)
            | ExpressionNode::Identifier(_)
            | ExpressionNode::SizeOf(_, _)
            | ExpressionNode::NameOf(_, _)
            | ExpressionNode::DeclOf(_, _)
            | ExpressionNode::SyntaxBlock(_) => expr.clone(),
        })
    }
}
