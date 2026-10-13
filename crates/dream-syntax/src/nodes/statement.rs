use super::expression::ExpressionNode;
use super::types::Type;
use crate::token::syntax_token::SyntaxToken;

/// Integer overflow behavior selected by a `checked { }` / `unchecked { }` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowMode {
    /// Overflow panics.
    Checked,
    /// Arithmetic wraps modulo the type's width (the default outside any block).
    Unchecked,
}

/// Represents a statement node in the AST
#[derive(Debug, Clone)]
pub enum StatementNode<'a> {
    Assignment(SyntaxToken, ExpressionNode<'a>),
    IndexAssignment(
        &'a ExpressionNode<'a>,
        &'a ExpressionNode<'a>,
        ExpressionNode<'a>,
    ),
    MemberAssignment(&'a ExpressionNode<'a>, SyntaxToken, ExpressionNode<'a>),
    /// `let`/`const` declaration. The final `bool` marks `const` (immutable) bindings.
    Declaration(SyntaxToken, Option<Type>, ExpressionNode<'a>, bool),
    /// `let (a, b) = expr;` / `const (a, (b, c)) = expr;` — positional tuple destructure.
    /// `pattern` is a [`crate::nodes::PatternNode::Tuple`] of irrefutable bindings/`_`/nested tuples.
    TupleDeclaration {
        pattern: crate::nodes::PatternNode,
        ty: Option<Type>,
        init: ExpressionNode<'a>,
        is_const: bool,
    },
    FunctionInvocation(SyntaxToken, Option<Vec<Type>>, Vec<ExpressionNode<'a>>),
    MethodInvocation(
        &'a ExpressionNode<'a>,
        SyntaxToken,
        Option<Vec<Type>>,
        Vec<ExpressionNode<'a>>,
    ),
    Return(Option<ExpressionNode<'a>>),
    Yield(SyntaxToken, Option<ExpressionNode<'a>>),
    AwaitForEach(SyntaxToken, &'a StatementNode<'a>),
    /// If condition, then body, else if pairs, else body
    IfElse(
        ExpressionNode<'a>,
        &'a [StatementNode<'a>],
        Vec<(ExpressionNode<'a>, &'a [StatementNode<'a>])>,
        Option<&'a [StatementNode<'a>]>,
    ),
    While(ExpressionNode<'a>, &'a [StatementNode<'a>]),
    /// `do { body } while (condition);` - the body always runs at least once.
    DoWhile(&'a [StatementNode<'a>], ExpressionNode<'a>),
    For(
        Option<&'a StatementNode<'a>>,
        Option<ExpressionNode<'a>>,
        Option<&'a StatementNode<'a>>,
        &'a [StatementNode<'a>],
    ),
    /// A labeled loop: `label: while (...) { ... }`. Wraps a single loop statement so that
    /// `break label;` / `continue label;` can target it.
    Labeled(String, &'a StatementNode<'a>),
    /// `break` / `continue`, optionally targeting an enclosing labeled loop.
    Break(Option<String>),
    Continue(Option<String>),
    /// An expression used as a statement, typically missing an assignment or call.
    ExpressionStatement(ExpressionNode<'a>),
    /// `<future-expr>.await;` used as a statement, discarding the resolved value. The inner
    /// expression produces the `Future` (it is NOT wrapped in `ExpressionNode::Await`).
    AwaitStmt(ExpressionNode<'a>),
    /// `for (let element in iterable) { body }`. Iterates the elements of an array. The two
    /// `String` fields are unique synthetic local names (index counter and array temp) generated
    /// by the parser so codegen can lower this to an index loop without re-evaluating `iterable`.
    ForEach(
        SyntaxToken,
        ExpressionNode<'a>,
        String,
        String,
        &'a [StatementNode<'a>],
    ),
    /// `switch (subject) { case labels: body ... default: body }`. Each case carries one or more
    /// constant label expressions and its own body; there is no implicit fallthrough. The final
    /// element is the optional `default` body.
    Switch(
        ExpressionNode<'a>,
        Vec<(Vec<ExpressionNode<'a>>, &'a [StatementNode<'a>])>,
        Option<&'a [StatementNode<'a>]>,
    ),
    /// `lock (target) { body }` — mutual exclusion on `target` (must be `shared class`-typed, or
    /// `Lock`), reentrant per-thread. See `docs/language` tasks/concurrency notes.
    Lock(ExpressionNode<'a>, &'a [StatementNode<'a>]),
    /// `defer { body }` / `defer(q) { body }` — opt-in deferred last-ref destroy. `q` is a `uint`
    /// drain budget (omitted: one chunk). See `docs/reference/language/memory.md`.
    Defer(Option<ExpressionNode<'a>>, &'a [StatementNode<'a>]),
    /// `checked { body }` / `unchecked { body }` — integer overflow behavior for arithmetic written
    /// lexically inside `body`. The token is the `checked`/`unchecked` keyword.
    Overflow(OverflowMode, SyntaxToken, &'a [StatementNode<'a>]),
}

impl StatementNode<'_> {
    pub fn contains_yield(&self) -> bool {
        let body = |statements: &[Self]| statements.iter().any(Self::contains_yield);
        match self {
            Self::Yield(..) => true,
            Self::IfElse(_, then_, elifs, else_) => {
                body(then_) || elifs.iter().any(|(_, b)| body(b)) || else_.is_some_and(body)
            }
            Self::While(_, b)
            | Self::DoWhile(b, _)
            | Self::ForEach(_, _, _, _, b)
            | Self::Lock(_, b)
            | Self::Defer(_, b)
            | Self::Overflow(_, _, b) => body(b),
            Self::For(init, _, step, b) => {
                init.is_some_and(Self::contains_yield)
                    || step.is_some_and(Self::contains_yield)
                    || body(b)
            }
            Self::Labeled(_, inner) | Self::AwaitForEach(_, inner) => inner.contains_yield(),
            Self::Switch(_, cases, default) => {
                cases.iter().any(|(_, b)| body(b)) || default.is_some_and(body)
            }
            Self::ExpressionStatement(ExpressionNode::Switch(_, _, arms)) => arms.iter().any(
                |arm| matches!(&arm.body, super::expression::SwitchArmBody::Block(b) if body(b)),
            ),
            _ => false,
        }
    }
}
