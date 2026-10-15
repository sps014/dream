use super::Analyzer;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{ExpressionNode, Type};
use dream_syntax::token::syntax_token::SyntaxToken;

impl<'a> Analyzer<'a> {
    /// Resolves a resource name to an owned byte array before MIR lowering.
    pub(super) fn analyze_asset_bytes(
        &mut self,
        params: &[ExpressionNode<'a>],
        method: &SyntaxToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, crate::errors::SemanticError> {
        let name = match params {
            [ExpressionNode::Literal(Type::String(t))] => {
                super::hir_emit::string_lit_value(&t.text)
            }
            _ => {
                diagnostics.report_error(
                    "Assets.bytes expects one string literal asset name".to_string(),
                    Some(method.position),
                );
                self.hir_fail();
                return Ok(Type::Unknown);
            }
        };
        let Some(bytes) = self.graph.embedded_assets.get(&name) else {
            diagnostics.report_error(
                format!("unknown embedded asset '{name}'"),
                Some(method.position),
            );
            self.hir_fail();
            return Ok(Type::Unknown);
        };
        let mut token = method.clone();
        token.text = "byte".to_string();
        let result = Type::Array(Box::new(Type::Byte(token)));
        let ty = self.type_ctx.interner.byte();
        let elements = bytes
            .iter()
            .map(|b| {
                Some(dream_hir::HExpr {
                    ty,
                    kind: dream_hir::HExprKind::IntLit(*b as i64),
                })
            })
            .collect();
        self.hir_set_array_lit(elements, &result);
        Ok(result)
    }
}
