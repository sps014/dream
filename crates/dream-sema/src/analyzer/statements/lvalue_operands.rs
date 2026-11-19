//! Single evaluation of assignment targets for `a[i] op= v`, `a[i]++`, `obj.f op= v` and
//! `obj.f++`. The parser desugars a compound assignment into `target = target op v` by cloning the
//! target, so its receiver and index would run twice. Each side-effecting operand is bound to a
//! fresh `const` first and the assignment is analyzed again over those bindings, which keeps the
//! ordinary indexer, property and setter paths in charge of the actual read and write.

use super::*;
use dream_syntax::nodes::{FunctionNode, StatementNode};

impl<'a> Analyzer<'a> {
    /// The rewritten form of an index or member assignment produced from a compound assignment,
    /// or `None` when the statement is a plain assignment or its target has no operand that needs
    /// hoisting. A compound assignment is recognized by its value: the parser builds it as
    /// `Binary(<clone of target>, op, rhs)`, and the clone shares the target's operand nodes.
    pub(in crate::analyzer) fn single_evaluation_assignment(
        &mut self,
        statement: &StatementNode<'a>,
        ctx: &AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<StatementNode<'a>>, SemanticError> {
        let (target, op, rhs): (&'a ExpressionNode<'a>, &SyntaxToken, &'a ExpressionNode<'a>) =
            match statement {
                StatementNode::IndexAssignment(arr, idx, ExpressionNode::Binary(l, op, rhs))
                    if matches!(l, ExpressionNode::IndexAccess(a, i)
                        if std::ptr::eq(*a, *arr) && std::ptr::eq(*i, *idx)) =>
                {
                    (l, op, rhs)
                }
                StatementNode::MemberAssignment(
                    obj,
                    member,
                    ExpressionNode::Binary(l, op, rhs),
                ) if matches!(l, ExpressionNode::MemberAccess(o, m)
                        if std::ptr::eq(*o, *obj) && m.text == member.text) =>
                {
                    (l, op, rhs)
                }
                _ => return Ok(None),
            };
        if !Self::place_needs_hoisting(target) {
            return Ok(None);
        }
        let target = self.hoist_place_operands(target, ctx, diagnostics)?;
        let value = ExpressionNode::Binary(target, op.clone(), rhs);
        Ok(Some(Self::assignment_to(target, value)))
    }

    /// Binds every side-effecting operand of the place `target` to a fresh `const` and returns the
    /// same place spelled over those bindings. Place prefixes (`a.b[i].c`) stay places so writes
    /// still reach the original storage.
    pub(in crate::analyzer) fn hoist_place_operands(
        &mut self,
        target: &'a ExpressionNode<'a>,
        ctx: &AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<&'a ExpressionNode<'a>, SemanticError> {
        Ok(match target {
            ExpressionNode::Identifier(_) => target,
            ExpressionNode::IndexAccess(base, index) => {
                let base = self.hoist_place_operands(base, ctx, diagnostics)?;
                let index = self.hoist_value(index, ctx, diagnostics)?;
                self.arena.alloc(ExpressionNode::IndexAccess(base, index))
            }
            ExpressionNode::MemberAccess(obj, member) => {
                let obj = self.hoist_place_operands(obj, ctx, diagnostics)?;
                self.arena
                    .alloc(ExpressionNode::MemberAccess(obj, member.clone()))
            }
            _ => self.hoist_value(target, ctx, diagnostics)?,
        })
    }

    /// True when some operand of the place `target` could have a side effect.
    pub(in crate::analyzer) fn place_needs_hoisting(target: &ExpressionNode<'a>) -> bool {
        match target {
            ExpressionNode::Identifier(_) => false,
            ExpressionNode::IndexAccess(base, index) => {
                Self::place_needs_hoisting(base) || !Self::is_pure_operand(index)
            }
            ExpressionNode::MemberAccess(obj, _) => Self::place_needs_hoisting(obj),
            _ => true,
        }
    }

    fn is_pure_operand(expr: &ExpressionNode<'a>) -> bool {
        matches!(
            expr,
            ExpressionNode::Identifier(_) | ExpressionNode::Literal(_)
        )
    }

    fn hoist_value(
        &mut self,
        value: &'a ExpressionNode<'a>,
        ctx: &AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<&'a ExpressionNode<'a>, SemanticError> {
        if Self::is_pure_operand(value) {
            return Ok(value);
        }
        let name = self.declare_synthetic_const("__lvalue", value, ctx, diagnostics)?;
        Ok(self.arena.alloc(ExpressionNode::Identifier(name)))
    }

    /// Declares `const <prefix>_<n> = value;` in the current scope and returns its name token.
    pub(in crate::analyzer) fn declare_synthetic_const(
        &mut self,
        prefix: &str,
        value: &ExpressionNode<'a>,
        ctx: &AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<SyntaxToken, SemanticError> {
        let name = synthetic_token(
            TokenKind::IdentifierToken,
            &format!("{prefix}_{}", self.lambda_counter),
        );
        self.lambda_counter += 1;
        self.analyze_declaration(&name, &None, value, (true, false), ctx, diagnostics)?;
        Ok(name)
    }

    /// `target = value;` for an assignable place.
    pub(in crate::analyzer) fn assignment_to(
        target: &'a ExpressionNode<'a>,
        value: ExpressionNode<'a>,
    ) -> StatementNode<'a> {
        match target {
            ExpressionNode::IndexAccess(arr, idx) => {
                StatementNode::IndexAssignment(arr, idx, value)
            }
            ExpressionNode::MemberAccess(obj, member) => {
                StatementNode::MemberAssignment(obj, member.clone(), value)
            }
            ExpressionNode::Identifier(id) => StatementNode::Assignment(id.clone(), value),
            _ => unreachable!("only places are assignment targets"),
        }
    }

    /// Analyzes a synthesized assignment without recording a debug line for it.
    pub(in crate::analyzer) fn analyze_synthetic_assignment(
        &mut self,
        statement: &StatementNode<'a>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let ctx = AnalyzerContext {
            parent_function,
            symbol_table,
        };
        if self.indexer_writeback_assignment(statement, &ctx, diagnostics)? {
            return Ok(());
        }
        match statement {
            StatementNode::IndexAssignment(arr, idx, value) => self.analyze_index_assignment(
                arr,
                idx,
                value,
                parent_function,
                symbol_table,
                diagnostics,
            ),
            StatementNode::MemberAssignment(obj, member, value) => self.analyze_member_assignment(
                obj,
                member,
                value,
                parent_function,
                symbol_table,
                diagnostics,
            ),
            StatementNode::Assignment(id, value) => {
                self.analyze_assignment(id, value, parent_function, symbol_table, diagnostics)
            }
            _ => unreachable!("only assignments are synthesized"),
        }
    }
}
