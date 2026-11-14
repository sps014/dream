//! `@main_loop` validation: the extern that owns the main thread on Android and iOS is called by
//! the generated `main` with the platform entry signature, so its declaration must match it
//! exactly and a program may link only one.

use super::*;
use dream_syntax::nodes::{FunctionNode, Type};

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn check_main_loop_declarations(
        &self,
        functions: &[&'a FunctionNode<'a>],
        diagnostics: &mut DiagnosticBag,
    ) {
        let mut first: Option<&FunctionNode<'_>> = None;
        for function in functions
            .iter()
            .filter(|f| dream_abi::attributes::has_main_loop_attr(&f.attributes))
        {
            diagnostics.file_path = file_path_string(&function.file_path);
            let at = Some(function.name.position);
            let types: Vec<&Type> = function.parameters.iter().map(|p| &p.type_).collect();
            let signature_ok = !function.is_async
                && function.generic_parameters.is_none()
                && function.parameters.iter().all(|p| !p.is_ref && !p.is_variadic)
                && matches!(
                    types.as_slice(),
                    [
                        Type::Integer(_),
                        Type::ISize(_),
                        Type::ISize(_),
                        Type::ISize(_)
                    ]
                )
                && matches!(function.return_type, Some(Type::Integer(_)));
            if !signature_ok {
                diagnostics.report_error(
                    "'@main_loop' externs must be declared \
                     `(argc: int, argv: isize, entry: isize, pump: isize): int`"
                        .into(),
                    at,
                );
            }
            match first {
                None => first = Some(function),
                Some(previous) => diagnostics.report_error(
                    format!(
                        "only one '@main_loop' function may be linked; '{}' already owns the main \
                         loop",
                        previous.name.text
                    ),
                    at,
                ),
            }
        }
    }
}
