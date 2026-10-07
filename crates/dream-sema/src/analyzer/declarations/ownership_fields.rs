//! Validates weak and unowned fields; recursive reference types are valid.
use super::*;
use dream_types::{TyKind, TypeId};
impl<'a> Analyzer<'a> {
    fn is_class_type(&self, ty: TypeId) -> bool {
        self.struct_info(ty).is_some_and(|info| !info.is_value)
    }
    pub(in crate::analyzer) fn validate_weak_unowned_fields(
        &mut self,
        node: &'a ProgramView<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        for struct_decl in node.structs.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(struct_decl.file_path.as_deref()));
            for field in &struct_decl.fields {
                if !field.is_weak && !field.is_unowned {
                    continue;
                }
                if field.is_weak && field.is_unowned {
                    diagnostics.report_error(
                        format!(
                            "field '{}' cannot be both 'weak' and 'unowned'",
                            field.name.text
                        ),
                        Some(field.name.position),
                    );
                    continue;
                }
                let ty = self.type_ctx.lower(&field.field_type);
                if field.is_weak {
                    let is_class_option = match self.type_ctx.interner.kind(ty) {
                        TyKind::Union(def, args) if args.len() == 1 => {
                            self.type_ctx.defs.name(*def) == "Option" && self.is_class_type(args[0])
                        }
                        _ => false,
                    };
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
                } else if !self.is_class_type(ty) {
                    diagnostics.report_error(
                        format!(
                            "'unowned' field '{}' must have a class type, got '{}'",
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
