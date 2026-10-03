//! On-the-fly monomorphization of a generic static method, dispatching the `System.print`,
//! `Buffer.alloc`, `Bytes.of`/`to`, `Promise.*`, and `Json.serialize`/`deserialize` intrinsics
//! before falling back to registering a plain generic-static instance.

use super::*;
use dream_abi::intrinsics;
use dream_syntax::nodes::types::is_unknown_type_name;

fn json_collection_write_fn(mangled: &str) -> Option<String> {
    json_collection_adapter(mangled, "write")
}

fn json_collection_de_fn(mangled: &str) -> Option<String> {
    json_collection_adapter(mangled, "de")
}

fn json_collection_adapter(mangled: &str, kind: &str) -> Option<String> {
    let base = mangled.trim_end_matches('?');
    let is_collection = base.ends_with("[]")
        || base.starts_with("List_")
        || base.starts_with("Set_")
        || base.starts_with("Map_string_")
        || base.starts_with("SortedMap_string_");
    if !is_collection {
        return None;
    }
    let suffix = base.replace("[]", "__arr");
    let method = format!("__col_{}_{}", kind, suffix);
    Some(dream_types::method_fn("Json", &method))
}

/// Call-site bundle for [`Analyzer::analyze_generic_static_method`]: the parsed pieces of a
/// `Type.method(args)` call already resolved to a generic static method template, kept together
/// so the analysis function itself only needs the bundle plus the analyzer context/diagnostics.
pub(super) struct GenericStaticMethodCall<'a, 'b> {
    pub(super) template: &'a FunctionNode<'a>,
    pub(super) base: &'b str,
    pub(super) type_name: &'b str,
    pub(super) method: &'b SyntaxToken,
    pub(super) generic_args: &'b Option<Vec<Type>>,
    pub(super) params: &'b Vec<ExpressionNode<'a>>,
}

mod buffer;
mod bytes;
mod json;

impl<'a> Analyzer<'a> {
    /// Resolves a `Type.method(args)` call whose `{Type}_{method}` names a generic static method
    /// (`call.template`). Handles the recognized intrinsics inline and otherwise registers a
    /// monomorphized instance. Always resolves to a type (the outer dispatch wraps it in `Some`);
    /// `call.base` is the mangled `{Type}_{method}` symbol and `call.type_name` the receiver
    /// type's name.
    pub(super) fn analyze_generic_static_method(
        &mut self,
        call: GenericStaticMethodCall<'a, '_>,
        ctx: &AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        let GenericStaticMethodCall {
            template,
            base,
            type_name,
            method,
            generic_args,
            params,
        } = call;
        let mut params_types = vec![];
        let mut arg_hirs = vec![];
        let call_target = format!("{}.{}", type_name, method.text);
        let saved_call_target = self.current_call_target_name.take();
        self.current_call_target_name = Some(call_target);

        let expected_params: Option<Vec<Type>> = {
            let has_lambda = params
                .iter()
                .any(|p| matches!(p, ExpressionNode::Lambda(_)));
            if has_lambda && generic_args.as_ref().is_none_or(|g| g.is_empty()) {
                let (paused_collecting, paused_ok) = self.hir_pause_collection();
                let mut probe = vec![String::new(); params.len()];
                for (i, param) in params.iter().enumerate() {
                    if matches!(param, ExpressionNode::Lambda(_)) {
                        continue;
                    }
                    if let Ok(t) = self.analyze_expression(
                        param,
                        ctx.parent_function,
                        ctx.symbol_table,
                        diagnostics,
                    ) {
                        probe[i] = t.get_type();
                    }
                    let _ = self.hir_take();
                }
                self.hir_resume_collection(paused_collecting, paused_ok);
                let gen_params = template.generic_parameters.as_deref().unwrap_or(&[]);
                let mut bindings = GenericBindings::new();
                for param in gen_params {
                    let concrete =
                        template
                            .parameters
                            .iter()
                            .enumerate()
                            .find_map(|(i, formal)| {
                                probe.get(i).filter(|s| !s.is_empty()).and_then(|arg| {
                                    Self::match_generic_type(&formal.type_, arg, &param.text)
                                })
                            });
                    if let Some(c) = concrete {
                        bindings.insert(param.text.clone(), Self::concrete_type_from_str(&c));
                    }
                }
                if bindings.is_empty() {
                    None
                } else {
                    Some(
                        template
                            .parameters
                            .iter()
                            .map(|p| {
                                Self::erase_unbound_generics(
                                    &Self::monomorphize_type(&p.type_, &bindings),
                                    &bindings,
                                    gen_params,
                                )
                            })
                            .collect(),
                    )
                }
            } else {
                None
            }
        };

        for (i, param) in params.iter().enumerate() {
            let saved_expected = self.current_expected_type.take();
            self.current_expected_type = expected_params.as_ref().and_then(|ps| ps.get(i).cloned());
            let t =
                self.analyze_expression(param, ctx.parent_function, ctx.symbol_table, diagnostics)?;
            self.current_expected_type = saved_expected;
            arg_hirs.push(self.hir_take());
            params_types.push(t.get_type());
        }
        self.current_call_target_name = saved_call_target;
        // `System.print`/`println` are generic builtins (not real monomorphizations): they lower
        // to the host `print_*` imports, so handle them before the generic-instance machinery.
        if let Some(op @ (intrinsics::IntrinsicOp::Print | intrinsics::IntrinsicOp::Println)) =
            intrinsics::IntrinsicOp::from_attributes(&template.attributes)
        {
            if params.len() != 1 {
                diagnostics.report_error(
                    format!(
                        "'{}' expects exactly 1 argument, got {}",
                        method.text,
                        params.len()
                    ),
                    Some(method.position),
                );
                self.hir_none();
            } else {
                let newline = op == intrinsics::IntrinsicOp::Println;
                self.hir_set_print(arg_hirs.into_iter().next().flatten(), newline);
                return Ok(Type::Void);
            }
            return Ok(Type::Unknown);
        }
        // Generic static calls / intrinsics need an `InstanceId` (a later slice); stay out of
        // HIR coverage regardless of which sub-branch handles the call.
        self.hir_none();
        if matches!(
            intrinsics::IntrinsicOp::from_attributes(&template.attributes),
            Some(
                intrinsics::IntrinsicOp::ArrayNew
                    | intrinsics::IntrinsicOp::ArrayRealloc
                    | intrinsics::IntrinsicOp::ArrayElemsCopy
                    | intrinsics::IntrinsicOp::ArrayElemsFill
                    | intrinsics::IntrinsicOp::ForceFree
                    | intrinsics::IntrinsicOp::ArrayGetUnchecked
                    | intrinsics::IntrinsicOp::ArraySetUnchecked
            )
        ) {
            return self.analyze_buffer_intrinsic(
                &GenericStaticMethodCall {
                    template,
                    base,
                    type_name,
                    method,
                    generic_args,
                    params,
                },
                params_types,
                arg_hirs,
                diagnostics,
            );
        }

        if matches!(
            intrinsics::IntrinsicOp::from_attributes(&template.attributes),
            Some(
                intrinsics::IntrinsicOp::ToBytes
                    | intrinsics::IntrinsicOp::FromBytes
                    | intrinsics::IntrinsicOp::WireEncode
                    | intrinsics::IntrinsicOp::WireDecode
            )
        ) {
            return self.analyze_bytes_intrinsic(
                &GenericStaticMethodCall {
                    template,
                    base,
                    type_name,
                    method,
                    generic_args,
                    params,
                },
                params_types,
                arg_hirs,
                diagnostics,
            );
        }

        let bindings = self.infer_generic_bindings(
            template,
            generic_args,
            &params_types,
            &method.position,
            diagnostics,
        );

        // Promise combinators (`Promise.all/any/race`) are typed by the shared async
        // intrinsic logic; classify via the registry and delegate when applicable.
        if let Some(combinator) = intrinsics::IntrinsicOp::from_attributes(&template.attributes)
            .and_then(|op| op.promise_combinator())
        {
            let mut s_tok = method.clone();
            s_tok.text = combinator.to_string();
            let ret = self.analyze_async_intrinsic(
                &s_tok,
                params,
                ctx.parent_function,
                ctx.symbol_table,
                diagnostics,
            )?;
            // `analyze_async_intrinsic` only types the combinator; its argument analysis leaves
            // the future-array HIR in `last`. Reuse it as the single arg of a direct call to the
            // combinator intrinsic so the MIR backend lowers it to `$dream_all/$dream_any`
            // (rather than emitting only the array, which would await the raw array pointer).
            let arg_hir = self.hir_take();
            self.hir_set_call(base, vec![arg_hir], &ret);
            return Ok(ret);
        }

        if matches!(
            intrinsics::IntrinsicOp::from_attributes(&template.attributes),
            Some(
                intrinsics::IntrinsicOp::JsonSerialize
                    | intrinsics::IntrinsicOp::JsonDeserialize
                    | intrinsics::IntrinsicOp::JsonFromValue
            )
        ) {
            return self.analyze_json_intrinsic(
                &GenericStaticMethodCall {
                    template,
                    base,
                    type_name,
                    method,
                    generic_args,
                    params,
                },
                params_types,
                arg_hirs,
                diagnostics,
            );
        }

        if !self.member_accessible(
            template.visibility,
            &template.file_path,
            ctx.parent_function.file_path.as_ref(),
            self.in_methods_of(ctx.parent_function, type_name),
        ) {
            diagnostics.report_error(
                format!(
                    "'{}' is private to '{}'",
                    method.text,
                    self.ty_str_display(type_name)
                ),
                Some(method.position),
            );
        }

        self.verify_generic_constraints(
            &template.generic_constraints,
            &bindings,
            &method.position,
            diagnostics,
        );
        let mangled_name = self.register_generic_function_instance(template, &bindings);

        let store_sig = match self.function_table.get_function(&mangled_name) {
            Ok(sig) => sig,
            Err(_) => {
                diagnostics.report_error(
                    format!("Function '{}' could not be instantiated", mangled_name),
                    Some(method.position),
                );
                return Ok(Type::Unknown);
            }
        };

        let required = store_sig.required_params();
        let total = store_sig.parameters.len();
        let given = params_types.len();
        if given < required || given > total {
            let message = if required == total {
                format!(
                    "Function {} has {} params but {} params are given",
                    mangled_name, total, given
                )
            } else {
                format!(
                    "Function {} expects between {} and {} arguments, got {}",
                    mangled_name, required, total, given
                )
            };
            diagnostics.report_error(message, Some(method.position));
            return Ok(Type::Unknown);
        }

        self.substitute_default_args(
            (&store_sig.defaults, &store_sig.parameter_types),
            &mut params_types,
            &mut arg_hirs,
            ctx.parent_function,
            ctx.symbol_table,
            diagnostics,
        )?;

        self.validate_arguments(
            &format!("function '{}'", mangled_name),
            &store_sig.parameters,
            &params_types,
            method.position,
            diagnostics,
        );

        let ret_type = Self::async_return_type(store_sig.is_async, store_sig.return_type);
        let instance = bindings.values().map(|t| self.type_ctx.lower(t)).collect();
        // `base` is the template's `{Type}_{method}` DefId shared by every monomorphization.
        self.hir_set_generic_call(
            base,
            instance,
            arg_hirs,
            &ret_type,
            store_sig.is_take.clone(),
        );
        Ok(ret_type)
    }
}
