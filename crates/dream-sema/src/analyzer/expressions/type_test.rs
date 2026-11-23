use super::*;
use crate::errors::SemanticError;
use dream_syntax::nodes::{ExpressionNode, Type};

impl<'a> Analyzer<'a> {
    /// Returns the folded result when the operand has a concrete static type.
    pub(in crate::analyzer) fn analyze_is_test(
        &mut self,
        operand: &ExpressionNode<'a>,
        target: &Type,
        binding: Option<&SyntaxToken>,
        ctx: &AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<bool>, SemanticError> {
        let operand_type = self.analyze_value_expression(
            operand,
            ctx.parent_function,
            ctx.symbol_table,
            diagnostics,
        )?;
        self.check_declared_type(
            target,
            &self
                .current_generic_bindings
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            diagnostics,
        );
        let value = self.hir_take();
        if operand_type.is_unknown() {
            return Ok(None);
        }
        let target = Self::monomorphize_type(target, &self.current_generic_bindings);
        let operand_id = value
            .as_ref()
            .map_or_else(|| self.type_ctx.lower(&operand_type), |hir| hir.ty);
        if operand_type.is_object() || self.is_interface_name(operand_id) {
            self.hir_set_is_type(value, &target, binding, None);
            return Ok(None);
        }
        self.check_is_possible(&operand_type, &target, operand.position(), diagnostics);
        let matches = operand_id == self.type_ctx.lower(&target);
        if binding.is_some() {
            self.hir_set_is_type(value, &target, binding, Some(matches));
        } else {
            self.hir_set_bool(matches);
        }
        Ok(Some(matches))
    }
}
