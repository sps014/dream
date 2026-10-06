use bumpalo::Bump;
use dream_syntax::nodes::ExpressionNode;
use dream_syntax::nodes::LambdaBody;
use dream_syntax::nodes::LambdaNode;
use dream_syntax::nodes::PatternNode;
use dream_syntax::nodes::StatementNode;
use dream_syntax::nodes::SwitchArm;
use dream_syntax::nodes::SwitchArmBody;
use dream_syntax::nodes::SyntaxBlockNode;
use dream_syntax::nodes::SyntaxBlockPart;
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_text::line_text::LineText;
use dream_text::text_span::TextSpan;

/// Maps wrapper-relative token positions onto the origin block in the user's file: each
/// point shifts by `delta` and clamps to `[lo, hi]`; line/col are recomputed against the
/// user's real source so rendered excerpts land inside their `html {}` block.
pub(super) struct SpanMap {
    pub(super) delta: isize,
    pub(super) lo: usize,
    pub(super) hi: usize,
    pub(super) line_text: LineText,
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
pub(super) fn shift_expr<'a>(
    map: &SpanMap,
    arena: &'a Bump,
    e: &ExpressionNode<'a>,
) -> ExpressionNode<'a> {
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
        ExpressionNode::Cast(t, ty, x) => ExpressionNode::Cast(
            map.token(t),
            ty.clone(),
            arena.alloc(shift_expr(map, arena, x)),
        ),
        ExpressionNode::SizeOf(t, ty) => ExpressionNode::SizeOf(map.token(t), ty.clone()),
        ExpressionNode::NameOf(t, path) => {
            ExpressionNode::NameOf(map.token(t), path.iter().map(|p| map.token(p)).collect())
        }
        ExpressionNode::DeclOf(t, path) => {
            ExpressionNode::DeclOf(map.token(t), path.iter().map(|p| map.token(p)).collect())
        }
        ExpressionNode::MemberAccess(x, t) => {
            ExpressionNode::MemberAccess(arena.alloc(shift_expr(map, arena, x)), map.token(t))
        }
        ExpressionNode::IsExpression(x, ty, bind) => ExpressionNode::IsExpression(
            arena.alloc(shift_expr(map, arena, x)),
            ty.clone(),
            bind.as_ref().map(|t| map.token(t)),
        ),
        ExpressionNode::TypeOf(kw, x) => {
            ExpressionNode::TypeOf(map.token(kw), arena.alloc(shift_expr(map, arena, x)))
        }
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
            let arms_v: Vec<SwitchArm> =
                arms.iter().map(|arm| shift_arm(map, arena, arm)).collect();
            ExpressionNode::Switch(map.token(t), subject, arms_v)
        }
        ExpressionNode::Try(x) => ExpressionNode::Try(arena.alloc(shift_expr(map, arena, x))),
        ExpressionNode::Lambda(l) => {
            ExpressionNode::Lambda(arena.alloc(shift_lambda(map, arena, l)))
        }
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

pub(super) fn shift_arm<'a>(map: &SpanMap, arena: &'a Bump, arm: &SwitchArm<'a>) -> SwitchArm<'a> {
    SwitchArm {
        pattern: shift_pattern(map, &arm.pattern),
        guard: arm.guard.as_ref().map(|g| shift_expr(map, arena, g)),
        body: match &arm.body {
            SwitchArmBody::Expr(e) => SwitchArmBody::Expr(shift_expr(map, arena, e)),
            SwitchArmBody::Block(b) => SwitchArmBody::Block(shift_stmts(map, arena, b)),
        },
    }
}

pub(super) fn shift_lambda<'a>(
    map: &SpanMap,
    arena: &'a Bump,
    l: &LambdaNode<'a>,
) -> LambdaNode<'a> {
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

pub(super) fn shift_pattern(map: &SpanMap, p: &PatternNode) -> PatternNode {
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

pub(super) fn shift_stmts<'a>(
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

pub(super) fn shift_stmt<'a>(
    map: &SpanMap,
    arena: &'a Bump,
    st: &StatementNode<'a>,
) -> StatementNode<'a> {
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
        StatementNode::Declaration(t, ty, e, c) => {
            StatementNode::Declaration(map.token(t), ty.clone(), shift_expr(map, arena, e), *c)
        }
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
                            labels.iter().map(|l| shift_expr(map, arena, l)).collect(),
                            shift_stmts(map, arena, body),
                        )
                    })
                    .collect();
                collected
            },
            default_b.map(|b| shift_stmts(map, arena, b)),
        ),
        StatementNode::Lock(target, body) => StatementNode::Lock(
            shift_expr(map, arena, target),
            shift_stmts(map, arena, body),
        ),
        StatementNode::Defer(q, body) => StatementNode::Defer(
            q.as_ref().map(|x| shift_expr(map, arena, x)),
            shift_stmts(map, arena, body),
        ),
        StatementNode::Overflow(mode, keyword, body) => {
            StatementNode::Overflow(*mode, map.token(keyword), shift_stmts(map, arena, body))
        }
    }
}
