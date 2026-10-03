use super::*;

impl<'a> Analyzer<'a> {
    pub(super) fn analyze_bytes_intrinsic(
        &mut self,
        call: &GenericStaticMethodCall<'a, '_>,
        params_types: Vec<dream_types::TypeId>,
        arg_hirs: Vec<Option<dream_hir::HExpr>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        let GenericStaticMethodCall {
            template,
            method,
            generic_args,
            ..
        } = *call;
        // `Bytes.of<T>(v)` / `Bytes.to<T>(bytes)`: raw byte-copy conversions between a blittable
        // value and a `byte[]` buffer (used by the worker-boundary adapter). `of` copies the
        // value's bytes out to a fresh buffer; `to` reconstructs a `T` from a buffer.
        let byte_op = intrinsics::IntrinsicOp::from_attributes(&template.attributes);
        if byte_op == Some(intrinsics::IntrinsicOp::ToBytes) {
            let named = |name: &str| -> Type {
                let mut t = method.clone();
                t.text = name.to_string();
                Type::from_token(t).unwrap_or(Type::Unknown)
            };
            if params_types.len() != 1 {
                diagnostics.report_error(
                    format!(
                        "'Bytes.of' expects exactly 1 argument (the value), got {}",
                        params_types.len()
                    ),
                    Some(method.position),
                );
            }
            let payload = match generic_args.as_ref().and_then(|g| g.first()) {
                Some(t) => Self::monomorphize_type(t, &self.current_generic_bindings),
                None => params_types
                    .first()
                    .map(|&ty| self.type_ctx.syntax_type(ty))
                    .unwrap_or(Type::Unknown),
            };
            self.require_unmanaged_or_array(&payload, "Bytes.of", &method.position, diagnostics);
            self.hir_set_to_bytes(arg_hirs.into_iter().next().flatten());
            return Ok(Type::Array(Box::new(named("byte"))));
        }
        if byte_op == Some(intrinsics::IntrinsicOp::FromBytes) {
            let target = match generic_args.as_ref().and_then(|g| g.first()) {
                Some(t) => Self::monomorphize_type(t, &self.current_generic_bindings),
                None => {
                    diagnostics.report_error(
                        "'Bytes.to' requires a type argument, e.g. Bytes.to<Point>(bytes)"
                            .to_string(),
                        Some(method.position),
                    );
                    Type::Void
                }
            };
            self.require_unmanaged_or_array(&target, "Bytes.to", &method.position, diagnostics);
            self.hir_set_from_bytes(&target, arg_hirs.into_iter().next().flatten());
            return Ok(target);
        }

        // `Bytes.toWire<T>(v)` / `Bytes.fromWire<T>(s)`: the `Task` wire marshal. `T = string`
        // is an identity passthrough (the wire already is a `string`); any other `T` must be
        // `unmanaged` and goes through a raw byte-blit (`Bytes.of`/`to`) re-encoded as a
        // codepoint-per-byte `string` (`Bytes.toWireString`/`fromWireString`).
        if byte_op == Some(intrinsics::IntrinsicOp::WireEncode) {
            let named = |name: &str| -> Type {
                let mut t = method.clone();
                t.text = name.to_string();
                Type::from_token(t).unwrap_or(Type::Unknown)
            };
            let payload = match generic_args.as_ref().and_then(|g| g.first()) {
                Some(t) => Self::monomorphize_type(t, &self.current_generic_bindings),
                None => params_types
                    .first()
                    .map(|&ty| self.type_ctx.syntax_type(ty))
                    .unwrap_or(Type::Unknown),
            };
            let value = arg_hirs.into_iter().next().flatten();
            // A still-abstract type parameter means this call sits inside a generic struct's own
            // declaration-time analysis pass (its methods are fully HIR-emitted once using the
            // class's type parameters as literal placeholder types, in addition to once per real
            // instantiation - unlike generic free functions, there is no "skip the unbound pass"
            // path for struct-level generics). That placeholder body is never actually reached by
            // any real call site, so its HIR just needs to type-check structurally: treat `T` as
            // `string` (identity passthrough) rather than trying to validate an unresolvable bound.
            if self.is_unresolved_generic_type(&payload) || payload.get_type() == "string" {
                self.hir_set_last(value);
            } else if payload.get_type() == "void" {
                if value.is_some() {
                    self.hir_expr_stmt(value);
                }
                let string_ty = named("string");
                let ty_id = self.type_ctx.lower(&string_ty);
                self.hir_set_last(Some(dream_hir::HExpr::new(
                    ty_id,
                    dream_hir::HExprKind::StringLit(String::new()),
                )));
            } else {
                self.require_unmanaged_or_array(
                    &payload,
                    "Bytes.toWire",
                    &method.position,
                    diagnostics,
                );
                self.hir_set_to_bytes(value);
                let bytes = self.hir_take();
                let bytes_ty = self.bytes_type();
                self.hir_set_type_method_call(bytes_ty, "toWireString", vec![bytes], &named("string"));
            }
            return Ok(named("string"));
        }
        if byte_op == Some(intrinsics::IntrinsicOp::WireDecode) {
            let target = match generic_args.as_ref().and_then(|g| g.first()) {
                Some(t) => Self::monomorphize_type(t, &self.current_generic_bindings),
                None => {
                    diagnostics.report_error(
                        "'Bytes.fromWire' requires a type argument, e.g. Bytes.fromWire<T>(text)"
                            .to_string(),
                        Some(method.position),
                    );
                    Type::Void
                }
            };
            let text = arg_hirs.into_iter().next().flatten();
            // See the matching comment in the `WireEncode` arm above: a dead placeholder body from
            // a generic struct's declaration-time analysis pass, never reached by a real call site.
            // `void` payloads travel as the empty wire string; passing it through keeps a
            // `Task.spawn` body returning `void` emittable without a value-less local.
            if self.is_unresolved_generic_type(&target)
                || matches!(target.get_type().as_str(), "string" | "void")
            {
                self.hir_set_last(text);
            } else {
                self.require_unmanaged_or_array(
                    &target,
                    "Bytes.fromWire",
                    &method.position,
                    diagnostics,
                );
                let named = |name: &str| -> Type {
                    let mut t = method.clone();
                    t.text = name.to_string();
                    Type::from_token(t).unwrap_or(Type::Unknown)
                };
                let bytes_ty = self.bytes_type();
                self.hir_set_type_method_call(
                    bytes_ty,
                    "fromWireString",
                    vec![text],
                    &Type::Array(Box::new(named("byte"))),
                );
                let bytes = self.hir_take();
                self.hir_set_from_bytes(&target, bytes);
            }
            return Ok(target);
        }

        crate::internal_error!("unclassified bytes intrinsic")
    }

    fn bytes_type(&self) -> dream_types::TypeId {
        self.type_ctx
            .resolved_type("Bytes")
            .unwrap_or_else(|| self.type_ctx.interner.error())
    }
}
