use super::*;

impl<'a> Analyzer<'a> {
    pub(super) fn analyze_buffer_intrinsic(
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
        // `Buffer.alloc<T>(len)`: a generic intrinsic that allocates a zero-initialized
        // `T[]`. The element type comes from the explicit type argument (resolved
        // through the active monomorphization bindings so `Buffer.alloc<T>` inside a
        // `List<int>` method yields `int[]`).
        if intrinsics::IntrinsicOp::from_attributes(&template.attributes)
            == Some(intrinsics::IntrinsicOp::ArrayNew)
        {
            let element = match generic_args.as_ref().and_then(|g| g.first()) {
                Some(t) => Self::monomorphize_type(t, &self.current_generic_bindings),
                None => {
                    diagnostics.report_error(
                        "'Buffer.alloc' requires a type argument, e.g. Buffer.alloc<int>(n)"
                            .to_string(),
                        Some(method.position),
                    );
                    Type::Void
                }
            };
            if params_types.len() != 1 {
                diagnostics.report_error(
                    format!(
                        "'Buffer.alloc' expects exactly 1 argument (length), got {}",
                        params_types.len()
                    ),
                    Some(method.position),
                );
            } else if params_types[0] != self.type_ctx.interner.int()
                && params_types[0] != self.type_ctx.interner.error()
            {
                diagnostics.report_error(
                    format!(
                        "'Buffer.alloc' length must be int, got {}",
                        self.type_id_display(params_types[0])
                    ),
                    Some(method.position),
                );
            }
            self.reject_ref_struct_array_element(&element, Some(method.position), diagnostics);
            self.hir_set_array_new(&element, arg_hirs.into_iter().next().flatten());
            return Ok(Type::Array(Box::new(element)));
        }

        // `Buffer.realloc<T>(arr, new_len)` (`@unsafe`): in-place `$realloc`-based grow/shrink of
        // `arr`'s backing block, returning a `T[]` of `new_len` elements.
        if intrinsics::IntrinsicOp::from_attributes(&template.attributes)
            == Some(intrinsics::IntrinsicOp::ArrayRealloc)
        {
            self.check_unsafe_intrinsic_call(
                "Buffer.realloc",
                template,
                method.position,
                diagnostics,
            );
            self.check_runtime_intrinsic_call(
                "Buffer.realloc",
                template,
                method.position,
                diagnostics,
            );
            let element = match generic_args.as_ref().and_then(|g| g.first()) {
                Some(t) => Self::monomorphize_type(t, &self.current_generic_bindings),
                None => params_types
                    .first()
                    .and_then(|&ty| match self.type_ctx.interner.kind(ty) {
                        dream_types::TyKind::Array(elem) => Some(self.type_ctx.syntax_type(*elem)),
                        _ => None,
                    })
                    .unwrap_or(Type::Unknown),
            };
            if params_types.len() != 2 {
                diagnostics.report_error(
                    format!(
                        "'Buffer.realloc' expects exactly 2 arguments (array, new length), got {}",
                        params_types.len()
                    ),
                    Some(method.position),
                );
            }
            let mut args = arg_hirs.into_iter();
            let array = args.next().flatten();
            let new_len = args.next().flatten();
            self.hir_set_array_realloc(&element, array, new_len);
            return Ok(Type::Array(Box::new(element)));
        }

        // `Buffer.elems_copy<T>(dst, dst_off, src, src_off, count)` (`@unsafe`): bulk blit of
        // unmanaged array elements via `memory.copy` (emitter supplies `sizeof(T)`).
        if intrinsics::IntrinsicOp::from_attributes(&template.attributes)
            == Some(intrinsics::IntrinsicOp::ArrayElemsCopy)
        {
            self.check_unsafe_intrinsic_call(
                "Buffer.elems_copy",
                template,
                method.position,
                diagnostics,
            );
            self.check_runtime_intrinsic_call(
                "Buffer.elems_copy",
                template,
                method.position,
                diagnostics,
            );
            let element = match generic_args.as_ref().and_then(|g| g.first()) {
                Some(t) => Self::monomorphize_type(t, &self.current_generic_bindings),
                None => {
                    diagnostics.report_error(
                        "'Buffer.elems_copy' requires a type argument, e.g. Buffer.elems_copy<int>(…)"
                            .to_string(),
                        Some(method.position),
                    );
                    Type::Unknown
                }
            };
            if !self.is_unresolved_generic_type(&element) {
                self.require_unmanaged(
                    &element,
                    "Buffer.elems_copy",
                    &method.position,
                    diagnostics,
                );
            }
            if params_types.len() != 5 {
                diagnostics.report_error(
                    format!(
                        "'Buffer.elems_copy' expects exactly 5 arguments (dst, dst_off, src, src_off, count), got {}",
                        params_types.len()
                    ),
                    Some(method.position),
                );
            }
            let mut args = arg_hirs.into_iter();
            let dst = args.next().flatten();
            let dst_off = args.next().flatten();
            let src = args.next().flatten();
            let src_off = args.next().flatten();
            let count = args.next().flatten();
            self.hir_set_array_elems_copy(&element, dst, dst_off, src, src_off, count);
            return Ok(Type::Void);
        }

        if intrinsics::IntrinsicOp::from_attributes(&template.attributes)
            == Some(intrinsics::IntrinsicOp::ArrayElemsFill)
        {
            self.check_unsafe_intrinsic_call(
                "Buffer.elems_fill",
                template,
                method.position,
                diagnostics,
            );
            self.check_runtime_intrinsic_call(
                "Buffer.elems_fill",
                template,
                method.position,
                diagnostics,
            );
            let element = match generic_args.as_ref().and_then(|g| g.first()) {
                Some(t) => Self::monomorphize_type(t, &self.current_generic_bindings),
                None => {
                    diagnostics.report_error(
                        "'Buffer.elems_fill' requires a type argument, e.g. Buffer.elems_fill<int>(…)"
                            .to_string(),
                        Some(method.position),
                    );
                    Type::Unknown
                }
            };
            if !self.is_unresolved_generic_type(&element) {
                self.require_unmanaged(
                    &element,
                    "Buffer.elems_fill",
                    &method.position,
                    diagnostics,
                );
            }
            if params_types.len() != 3 {
                diagnostics.report_error(
                    format!(
                        "'Buffer.elems_fill' expects exactly 3 arguments (dst, dst_off, count), got {}",
                        params_types.len()
                    ),
                    Some(method.position),
                );
            }
            let mut args = arg_hirs.into_iter();
            let dst = args.next().flatten();
            let dst_off = args.next().flatten();
            let count = args.next().flatten();
            self.hir_set_array_elems_fill(&element, dst, dst_off, count);
            return Ok(Type::Void);
        }

        // `Buffer.free<T>(arr)` (`@unsafe`): unconditionally returns `arr`'s backing block to the
        // allocator, bypassing reference counting.
        if intrinsics::IntrinsicOp::from_attributes(&template.attributes)
            == Some(intrinsics::IntrinsicOp::ForceFree)
        {
            self.check_unsafe_intrinsic_call("Buffer.free", template, method.position, diagnostics);
            self.check_runtime_intrinsic_call(
                "Buffer.free",
                template,
                method.position,
                diagnostics,
            );
            if params_types.len() != 1 {
                diagnostics.report_error(
                    format!(
                        "'Buffer.free' expects exactly 1 argument (the array), got {}",
                        params_types.len()
                    ),
                    Some(method.position),
                );
            }
            self.hir_set_force_free(arg_hirs.into_iter().next().flatten());
            return Ok(Type::Void);
        }

        // `Buffer.get_unchecked<T>(arr, i)` / `Buffer.set_unchecked<T>(arr, i, v)` (`@unsafe`):
        // element access whose bounds the caller has already established.
        if let Some(
            op @ (intrinsics::IntrinsicOp::ArrayGetUnchecked
            | intrinsics::IntrinsicOp::ArraySetUnchecked),
        ) = intrinsics::IntrinsicOp::from_attributes(&template.attributes)
        {
            let is_set = op == intrinsics::IntrinsicOp::ArraySetUnchecked;
            let name = if is_set {
                "Buffer.set_unchecked"
            } else {
                "Buffer.get_unchecked"
            };
            self.check_unsafe_intrinsic_call(name, template, method.position, diagnostics);
            let arity = if is_set { 3 } else { 2 };
            if params_types.len() != arity {
                diagnostics.report_error(
                    format!(
                        "'{}' expects exactly {} arguments, got {}",
                        name,
                        arity,
                        params_types.len()
                    ),
                    Some(method.position),
                );
                return Ok(Type::Unknown);
            }
            let element = match self.type_ctx.interner.kind(params_types[0]) {
                dream_types::TyKind::Array(elem) => self.type_ctx.syntax_type(*elem),
                dream_types::TyKind::Error => Type::Unknown,
                _ => {
                    diagnostics.report_error(
                        format!(
                            "'{}' expects an array as its first argument, got {}",
                            name,
                            self.type_id_display(params_types[0])
                        ),
                        Some(method.position),
                    );
                    return Ok(Type::Unknown);
                }
            };
            let mut args = arg_hirs.into_iter();
            let array = args.next().flatten();
            let index = args.next().flatten();
            if is_set {
                let value = args.next().flatten();
                self.hir_set_array_set_unchecked(array, index, value);
                return Ok(Type::Void);
            }
            self.hir_set_array_get_unchecked(&element, array, index);
            return Ok(element);
        }

        crate::internal_error!("unclassified buffer intrinsic")
    }
}
