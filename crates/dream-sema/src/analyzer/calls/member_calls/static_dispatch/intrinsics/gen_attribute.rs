use super::*;

impl<'a> Analyzer<'a> {
    /// `GenAttributes.id<A>()` / `decode<A>(attrs)` / `decode_all<A>(attrs)`: forwards to the
    /// `__gen_*` statics synthesized on `@attribute` type `A`, so a generic generator helper
    /// (`node.attribute<A>()`) reads typed attribute values without string lookups.
    pub(super) fn analyze_gen_attribute_intrinsic(
        &mut self,
        call: &GenericStaticMethodCall<'a, '_>,
        op: intrinsics::IntrinsicOp,
        arg_hirs: Vec<Option<dream_hir::HExpr>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        use dream_syntax::token::token_kind::TokenKind;

        let method = call.method;
        let Some(attr_ty) = call
            .generic_args
            .as_ref()
            .and_then(|g| g.first())
            .map(|t| Self::monomorphize_type(t, &self.current_generic_bindings))
        else {
            diagnostics.report_error(
                format!(
                    "'GenAttributes.{}' requires an attribute type argument",
                    method.text
                ),
                Some(method.position),
            );
            self.hir_fail();
            self.hir_none();
            return Ok(Type::Unknown);
        };
        let attr_id = self.type_ctx.lower(&attr_ty);
        let member = match op {
            intrinsics::IntrinsicOp::GenAttributeId => intrinsics::GEN_ATTRIBUTE_ID_MEMBER,
            intrinsics::IntrinsicOp::GenAttributeDecode => intrinsics::GEN_ATTRIBUTE_DECODE_MEMBER,
            _ => intrinsics::GEN_ATTRIBUTE_DECODE_ALL_MEMBER,
        };
        let decoder = self
            .function_table
            .generic_methods
            .get(&(attr_id, member.to_string()))
            .and_then(|def| self.generic_functions.get(def))
            .copied();
        if decoder.is_none()
            && self
                .function_table
                .method_candidates(attr_id, member)
                .is_empty()
        {
            diagnostics.report_error(
                format!(
                    "'{}' is not an @attribute type (declare it with '@attribute struct {}' and import 'system.codegen')",
                    self.type_id_display(attr_id),
                    attr_ty.display_name()
                ),
                Some(method.position),
            );
            self.hir_fail();
            self.hir_none();
            return Ok(Type::Unknown);
        }
        let wrap = |name: &str| {
            Type::Struct(
                synthetic_token(TokenKind::IdentifierToken, name),
                Some(vec![attr_ty.clone()]),
            )
        };
        let ret = match op {
            intrinsics::IntrinsicOp::GenAttributeId => Self::type_from_name("string"),
            intrinsics::IntrinsicOp::GenAttributeDecode => {
                self.ensure_union_instantiated(
                    "Option",
                    std::slice::from_ref(&attr_ty),
                    &method.position,
                    diagnostics,
                );
                wrap("Option")
            }
            _ => {
                self.ensure_struct_instantiated(
                    "List",
                    std::slice::from_ref(&attr_ty),
                    &method.position,
                    diagnostics,
                );
                wrap("List")
            }
        };
        if let Some(template) = decoder {
            // Instantiating only requested decoders avoids eagerly expanding List<A> for every
            // compiler attribute whenever a program imports system.codegen.
            let bindings = [(
                intrinsics::GEN_ATTRIBUTE_DECODER_PARAM.to_string(),
                Self::type_from_name("int"),
            )]
            .into();
            let identity = self.register_generic_function_instance(template, &bindings);
            self.hir_set_call_identity(&identity, arg_hirs, &ret);
        } else {
            self.hir_set_type_method_call(attr_id, member, arg_hirs, &ret);
        }
        Ok(ret)
    }
}
