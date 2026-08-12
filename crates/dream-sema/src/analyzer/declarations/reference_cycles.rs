//! Validation of `weak` field modifiers.
//!
//! Under the tracing GC, strong reference cycles are collected normally — there is no
//! compile-time cycle checker and no `@allow_cycle` escape hatch. This module only enforces
//! the shape of `weak` fields (`Option<T>` for a class `T`).

use super::*;

impl<'a> Analyzer<'a> {
    /// Validates every `weak` field's shape. Called from [`Self::register_structs`] once every
    /// non-generic class is registered in `self.struct_table` (so field types can be classified
    /// as value/reference).
    pub(in crate::analyzer) fn check_weak_fields(
        &self,
        node: &'a ProgramNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        for struct_decl in node.structs.iter() {
            for field in &struct_decl.fields {
                if !field.is_weak {
                    continue;
                }
                let option_inner = match &field.field_type {
                    Type::Struct(token, Some(args))
                        if token.text == "Option" && args.len() == 1 =>
                    {
                        Some(&args[0])
                    }
                    _ => None,
                };
                let is_class_option = option_inner
                    .and_then(Self::resolve_struct_parts)
                    .and_then(|(base, _)| {
                        self.struct_table.get_struct(&base).map(|i| !i.is_value)
                    })
                    .unwrap_or(false);
                if !is_class_option {
                    diagnostics.report_error(
                        format!(
                            "'weak' field '{}' must have type 'Option<T>' where 'T' is a class, got '{}'",
                            field.name.text,
                            field.field_type.display_name()
                        ),
                        Some(field.name.position),
                    );
                }
            }
        }
    }
}
