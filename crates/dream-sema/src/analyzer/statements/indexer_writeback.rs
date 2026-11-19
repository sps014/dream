//! Writes through an indexer that returns a value-type copy, such as `points[0].x = 5` on a
//! `List<Point>`. The getter hands back a copy, so the write lands in a temporary that is then
//! stored back through the indexer setter: `let t = points[0]; t.x = 5; points[0] = t;`.
//! A method called on the element is written back the same way, since it may mutate the copy.

use super::*;
use crate::analyzer::declarations::protocol_hooks::ProtocolRole;
use dream_syntax::nodes::StatementNode;

impl<'a> Analyzer<'a> {
    /// Analyzes `statement` with write-back when its target goes through a value-copy indexer.
    /// Returns `false`, having analyzed nothing, when the target needs no write-back.
    pub(in crate::analyzer) fn indexer_writeback_assignment(
        &mut self,
        statement: &StatementNode<'a>,
        ctx: &AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<bool, SemanticError> {
        let container = match statement {
            StatementNode::MemberAssignment(obj, _, _) => *obj,
            StatementNode::IndexAssignment(arr, _, _) => *arr,
            StatementNode::MethodInvocation(obj, ..) => *obj,
            _ => return Ok(false),
        };
        let Some(access) = self.value_copy_access(container, ctx, diagnostics) else {
            return Ok(false);
        };
        let ExpressionNode::IndexAccess(base, index) =
            self.hoist_place_operands(access, ctx, diagnostics)?
        else {
            unreachable!("a hoisted index access stays an index access")
        };
        let copy = synthetic_token(
            TokenKind::IdentifierToken,
            &format!("__element_{}", self.lambda_counter),
        );
        self.lambda_counter += 1;
        let read = self.arena.alloc(ExpressionNode::IndexAccess(base, index));
        self.analyze_declaration(&copy, &None, read, (false, false), ctx, diagnostics)?;

        let copy_ref: &'a ExpressionNode<'a> =
            self.arena.alloc(ExpressionNode::Identifier(copy.clone()));
        let target = self.replace_in_place(container, access, copy_ref);
        match statement {
            StatementNode::MemberAssignment(_, member, value) => {
                let write = StatementNode::MemberAssignment(target, member.clone(), value.clone());
                self.analyze_synthetic_assignment(
                    &write,
                    ctx.parent_function,
                    ctx.symbol_table,
                    diagnostics,
                )?;
            }
            StatementNode::IndexAssignment(_, idx, value) => {
                let write = StatementNode::IndexAssignment(target, idx, value.clone());
                self.analyze_synthetic_assignment(
                    &write,
                    ctx.parent_function,
                    ctx.symbol_table,
                    diagnostics,
                )?;
            }
            StatementNode::MethodInvocation(_, name, generics, args) => {
                let call = StatementNode::MethodInvocation(
                    target,
                    name.clone(),
                    generics.clone(),
                    args.clone(),
                );
                self.analyze_statement(
                    &call,
                    ctx.parent_function,
                    ctx.symbol_table,
                    false,
                    diagnostics,
                )?;
            }
            _ => unreachable!("only assignments and method calls reach indexer write-back"),
        }
        let store = StatementNode::IndexAssignment(base, index, ExpressionNode::Identifier(copy));
        self.analyze_synthetic_assignment(
            &store,
            ctx.parent_function,
            ctx.symbol_table,
            diagnostics,
        )?;
        Ok(true)
    }

    /// The index access nearest the written place whose indexer returns a value-type copy and
    /// has a setter to store it back.
    fn value_copy_access(
        &mut self,
        container: &'a ExpressionNode<'a>,
        ctx: &AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Option<&'a ExpressionNode<'a>> {
        let mut node = container;
        loop {
            match node {
                ExpressionNode::IndexAccess(base, _) => {
                    if self.reads_value_copy(base, node, ctx, diagnostics) {
                        return Some(node);
                    }
                    node = base;
                }
                ExpressionNode::MemberAccess(obj, _) => node = obj,
                _ => return None,
            }
        }
    }

    fn reads_value_copy(
        &mut self,
        base: &'a ExpressionNode<'a>,
        access: &'a ExpressionNode<'a>,
        ctx: &AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> bool {
        let base_type = self.type_of_place(base, ctx, diagnostics);
        if matches!(base_type, Type::Array(_) | Type::Unknown | Type::String(_)) {
            return false;
        }
        let has_setter = self
            .interface_protocol_method(&base_type, ProtocolRole::Set)
            .is_some()
            || self
                .protocol_hook(&base_type, ProtocolRole::Set, diagnostics)
                .is_some();
        if !has_setter {
            return false;
        }
        let element = self.type_of_place(access, ctx, diagnostics);
        let element = self.type_ctx.lower(&element);
        self.type_ctx.interner.is_value_type(element)
    }

    /// The type of a place prefix. Its HIR is discarded: the real read is analyzed again once the
    /// write-back form is built.
    fn type_of_place(
        &mut self,
        place: &'a ExpressionNode<'a>,
        ctx: &AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Type {
        let ty = self
            .analyze_expression(place, ctx.parent_function, ctx.symbol_table, diagnostics)
            .unwrap_or(Type::Unknown);
        let _ = self.hir_take();
        ty
    }

    /// `place` with the node `old` (compared by address) replaced by `new`.
    fn replace_in_place(
        &mut self,
        place: &'a ExpressionNode<'a>,
        old: &'a ExpressionNode<'a>,
        new: &'a ExpressionNode<'a>,
    ) -> &'a ExpressionNode<'a> {
        if std::ptr::eq(place, old) {
            return new;
        }
        match place {
            ExpressionNode::MemberAccess(obj, member) => {
                let obj = self.replace_in_place(obj, old, new);
                self.arena
                    .alloc(ExpressionNode::MemberAccess(obj, member.clone()))
            }
            ExpressionNode::IndexAccess(base, index) => {
                let base = self.replace_in_place(base, old, new);
                self.arena.alloc(ExpressionNode::IndexAccess(base, index))
            }
            _ => place,
        }
    }
}
