use super::*;

impl<'a> Analyzer<'a> {
    /// Ensures a `public` function does not leak a private (non-`public`) type through its
    /// signature, including nested generics (`List<Private>`, `Option<Private>`), tuples, arrays,
    /// function types, and private enums/unions/interfaces.
    pub(in crate::analyzer) fn check_public_visibility(
        &self,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let signature_types = function
            .return_type
            .iter()
            .chain(function.parameters.iter().map(|p| &p.type_));
        for type_to_check in signature_types {
            self.check_public_type_exposed(type_to_check, function, diagnostics);
        }
    }

    pub(in crate::analyzer) fn check_public_type_exposed(
        &self,
        ty: &Type,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        match ty {
            Type::Array(inner) => self.check_public_type_exposed(inner, function, diagnostics),
            Type::Tuple(elems) => {
                for e in elems {
                    self.check_public_type_exposed(e, function, diagnostics);
                }
            }
            Type::Function(params, ret) => {
                for p in params {
                    self.check_public_type_exposed(p, function, diagnostics);
                }
                self.check_public_type_exposed(ret, function, diagnostics);
            }
            Type::Struct(_token, args) => {
                self.check_nominal_not_private(ty, function, diagnostics);
                if let Some(args) = args {
                    for a in args {
                        self.check_public_type_exposed(a, function, diagnostics);
                    }
                }
            }
            _ => {}
        }
    }

    pub(in crate::analyzer) fn check_nominal_not_private(
        &self,
        ty: &Type,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let Type::Struct(token, _) = ty else {
            return;
        };
        let name = &token.text;
        if let Some(struct_info) = self
            .type_ctx
            .lookup_type(ty)
            .and_then(|ty| self.struct_info(ty))
            && !struct_info.visibility.is_public()
        {
            diagnostics.report_error(
                format!(
                    "Public function '{}' exposes private class '{}'",
                    function.name.text, name
                ),
                Some(function.name.position),
            );
        }
        let def = self
            .type_ctx
            .nominal_kind(name)
            .and_then(|kind| self.type_ctx.resolve(kind, name));
        if let Some((_, visibility)) = def.and_then(|def| self.type_visibility.get(&def))
            && !visibility.is_public()
        {
            diagnostics.report_error(
                format!(
                    "Public function '{}' exposes private type '{}'",
                    function.name.text, name
                ),
                Some(function.name.position),
            );
        }
    }
}
