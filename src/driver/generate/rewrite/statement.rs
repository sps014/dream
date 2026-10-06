use super::functions::rewrite_function_body;
use super::model::Rewriter;
use dream_syntax::nodes::StatementNode;
use std::io::Error;

impl<'a> Rewriter<'a, '_> {
    pub(super) fn statement(
        &mut self,
        stmt: &StatementNode<'a>,
    ) -> Result<StatementNode<'a>, Error> {
        let arena = self.arena;
        Ok(match stmt {
            StatementNode::ExpressionStatement(e) => {
                StatementNode::ExpressionStatement(self.expression(e)?)
            }
            StatementNode::AwaitStmt(e) => StatementNode::AwaitStmt(self.expression(e)?),
            StatementNode::Return(Some(e)) => StatementNode::Return(Some(self.expression(e)?)),
            StatementNode::Return(None) => StatementNode::Return(None),
            StatementNode::Assignment(n, e) => {
                StatementNode::Assignment(n.clone(), self.expression(e)?)
            }
            StatementNode::Declaration(n, ty, e, c) => {
                StatementNode::Declaration(n.clone(), ty.clone(), self.expression(e)?, *c)
            }
            StatementNode::TupleDeclaration {
                pattern,
                ty,
                init,
                is_const,
            } => StatementNode::TupleDeclaration {
                pattern: pattern.clone(),
                ty: ty.clone(),
                init: self.expression(init)?,
                is_const: *is_const,
            },
            StatementNode::IndexAssignment(a, i, v) => StatementNode::IndexAssignment(
                arena.alloc(self.expression(a)?),
                arena.alloc(self.expression(i)?),
                self.expression(v)?,
            ),
            StatementNode::MemberAssignment(r, m, v) => StatementNode::MemberAssignment(
                arena.alloc(self.expression(r)?),
                m.clone(),
                self.expression(v)?,
            ),
            StatementNode::FunctionInvocation(n, g, args) => {
                let mut nargs = Vec::new();
                for a in args {
                    nargs.push(self.expression(a)?);
                }
                StatementNode::FunctionInvocation(n.clone(), g.clone(), nargs)
            }
            StatementNode::MethodInvocation(r, n, g, args) => {
                let mut nargs = Vec::new();
                for a in args {
                    nargs.push(self.expression(a)?);
                }
                StatementNode::MethodInvocation(
                    arena.alloc(self.expression(r)?),
                    n.clone(),
                    g.clone(),
                    nargs,
                )
            }
            StatementNode::IfElse(cond, then_b, elifs, else_b) => {
                let ncond = self.expression(cond)?;
                let nthen = rewrite_function_body(
                    arena,
                    then_b,
                    self.by_site,
                    self.diagnostics,
                    self.file,
                    self.file_contents,
                )?;
                let mut nelifs = Vec::new();
                for (c, b) in elifs {
                    nelifs.push((
                        self.expression(c)?,
                        rewrite_function_body(
                            arena,
                            b,
                            self.by_site,
                            self.diagnostics,
                            self.file,
                            self.file_contents,
                        )?,
                    ));
                }
                let nelse = match else_b {
                    Some(b) => Some(rewrite_function_body(
                        arena,
                        b,
                        self.by_site,
                        self.diagnostics,
                        self.file,
                        self.file_contents,
                    )?),
                    None => None,
                };
                StatementNode::IfElse(ncond, nthen, nelifs, nelse)
            }
            StatementNode::While(cond, body) => StatementNode::While(
                self.expression(cond)?,
                rewrite_function_body(
                    arena,
                    body,
                    self.by_site,
                    self.diagnostics,
                    self.file,
                    self.file_contents,
                )?,
            ),
            StatementNode::Lock(cond, body) => StatementNode::Lock(
                self.expression(cond)?,
                rewrite_function_body(
                    arena,
                    body,
                    self.by_site,
                    self.diagnostics,
                    self.file,
                    self.file_contents,
                )?,
            ),
            StatementNode::Overflow(mode, keyword, body) => StatementNode::Overflow(
                *mode,
                keyword.clone(),
                rewrite_function_body(
                    arena,
                    body,
                    self.by_site,
                    self.diagnostics,
                    self.file,
                    self.file_contents,
                )?,
            ),
            StatementNode::Defer(budget, body) => StatementNode::Defer(
                match budget {
                    Some(q) => Some(self.expression(q)?),
                    None => None,
                },
                rewrite_function_body(
                    arena,
                    body,
                    self.by_site,
                    self.diagnostics,
                    self.file,
                    self.file_contents,
                )?,
            ),
            StatementNode::DoWhile(body, cond) => StatementNode::DoWhile(
                rewrite_function_body(
                    arena,
                    body,
                    self.by_site,
                    self.diagnostics,
                    self.file,
                    self.file_contents,
                )?,
                self.expression(cond)?,
            ),
            StatementNode::For(init, cond, inc, body) => {
                let ninit = match init {
                    Some(s) => Some(&*arena.alloc(self.statement(s)?)),
                    None => None,
                };
                let ncond = match cond {
                    Some(e) => Some(self.expression(e)?),
                    None => None,
                };
                let ninc = match inc {
                    Some(s) => Some(&*arena.alloc(self.statement(s)?)),
                    None => None,
                };
                StatementNode::For(
                    ninit,
                    ncond,
                    ninc,
                    rewrite_function_body(
                        arena,
                        body,
                        self.by_site,
                        self.diagnostics,
                        self.file,
                        self.file_contents,
                    )?,
                )
            }
            StatementNode::ForEach(n, iter, a, b, body) => StatementNode::ForEach(
                n.clone(),
                self.expression(iter)?,
                a.clone(),
                b.clone(),
                rewrite_function_body(
                    arena,
                    body,
                    self.by_site,
                    self.diagnostics,
                    self.file,
                    self.file_contents,
                )?,
            ),
            StatementNode::Switch(subj, cases, default) => {
                let nsubj = self.expression(subj)?;
                let mut ncases = Vec::new();
                for (labels, body) in cases {
                    let mut nlabels = Vec::new();
                    for l in labels {
                        nlabels.push(self.expression(l)?);
                    }
                    ncases.push((
                        nlabels,
                        rewrite_function_body(
                            arena,
                            body,
                            self.by_site,
                            self.diagnostics,
                            self.file,
                            self.file_contents,
                        )?,
                    ));
                }
                let ndef = match default {
                    Some(b) => Some(rewrite_function_body(
                        arena,
                        b,
                        self.by_site,
                        self.diagnostics,
                        self.file,
                        self.file_contents,
                    )?),
                    None => None,
                };
                StatementNode::Switch(nsubj, ncases, ndef)
            }
            StatementNode::Labeled(l, inner) => {
                StatementNode::Labeled(l.clone(), arena.alloc(self.statement(inner)?))
            }
            StatementNode::Break(x) => StatementNode::Break(x.clone()),
            StatementNode::Continue(x) => StatementNode::Continue(x.clone()),
        })
    }
}
