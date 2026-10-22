//! Promise combinators: `all(xs): Future<T[]>` and `race(xs): Future<T>`.

use super::*;
use dream_abi::intrinsics;
use dream_syntax::nodes::ExpressionNode;

impl<'a> Analyzer<'a> {
    /// Types `all(xs: Future<T>[]): Future<T[]>` and `race(xs: Future<T>[]): Future<T>`.
    pub(crate) fn analyze_async_intrinsic(
        &mut self,
        combinator: intrinsics::IntrinsicOp,
        name: &SyntaxToken,
        params: &[ExpressionNode<'a>],
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        // all/race take a single `Future<T>[]` argument.
        if params.len() != 1 {
            diagnostics.report_error(
                format!(
                    "'{}' expects exactly 1 argument (a Future array), got {}",
                    name.text,
                    params.len()
                ),
                Some(name.position),
            );
            return Ok(Type::Unknown);
        }
        let arg_type =
            self.analyze_expression(&params[0], parent_function, symbol_table, diagnostics)?;
        let inner_t = match &arg_type {
            Type::Array(inner) => match Self::future_inner_type(inner) {
                Some(t) => t,
                None => {
                    diagnostics.report_error(
                        format!(
                            "'{}' expects an array of Future values, got {}",
                            name.text,
                            self.ty_display(&arg_type)
                        ),
                        params[0].position(),
                    );
                    Type::Void
                }
            },
            _ => {
                diagnostics.report_error(
                    format!(
                        "'{}' expects an array of Future values, got {}",
                        name.text,
                        self.ty_display(&arg_type)
                    ),
                    params[0].position(),
                );
                Type::Void
            }
        };
        if combinator == intrinsics::IntrinsicOp::PromiseAll {
            // Future<T[]>
            Ok(Self::future_type(Type::Array(Box::new(inner_t))))
        } else {
            // race -> Future<T>
            Ok(Self::future_type(inner_t))
        }
    }
}
