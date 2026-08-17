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
            let has_lambda = params.iter().any(|p| matches!(p, ExpressionNode::Lambda(_)));
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
                    let concrete = template.parameters.iter().enumerate().find_map(|(i, formal)| {
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
            }
            return Ok(Type::Unknown);
        }
        // Generic static calls / intrinsics need an `InstanceId` (a later slice); stay out of
        // HIR coverage regardless of which sub-branch handles the call.
        self.hir_none();
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
            } else if params_types[0] != "int" && !is_unknown_type_name(&params_types[0]) {
                diagnostics.report_error(
                    format!("'Buffer.alloc' length must be int, got {}", params_types[0]),
                    Some(method.position),
                );
            }
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
                    .map(|s| s.trim_end_matches("[]").to_string())
                    .map(|s| {
                        let mut t = method.clone();
                        t.text = s;
                        Type::from_token(t).unwrap_or(Type::Unknown)
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
                    .map(|s| named(s))
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

        // `Bytes.toWire<T>(v)` / `Bytes.fromWire<T>(s)`: the `WebWorker` wire marshal. `T = string`
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
                    .map(|s| named(s))
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
                self.hir_set_call("Bytes_toWireString", vec![bytes], &named("string"));
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
            if self.is_unresolved_generic_type(&target) || target.get_type() == "string" {
                self.hir_set_last(text);
            } else if target.get_type() == "void" {
                self.hir_none();
                return Ok(Type::Void);
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
                self.hir_set_call(
                    "Bytes_fromWireString",
                    vec![text],
                    &Type::Array(Box::new(named("byte"))),
                );
                let bytes = self.hir_take();
                self.hir_set_from_bytes(&target, bytes);
            }
            return Ok(target);
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

        // `Json.serialize<T>(v)` / `Json.deserialize<T>(text)`: the `@json` derive emits
        // `<T>.write_json(sb)` / `<T>.from_json()` / `<T>.from_json_parser_text()` (see
        // `driver::generate` / Dream `JsonGenerator`). Expand the intrinsic into that composition
        // so the whole thing lowers through MIR as ordinary calls. Classes/structs deserialize
        // through the typed parser (no `JsonValue` tree); unions and collection `T` keep parse +
        // `from_json`.
        let json_op = intrinsics::IntrinsicOp::from_attributes(&template.attributes);
        if json_op == Some(intrinsics::IntrinsicOp::JsonSerialize) {
            use dream_hir::{Binding, HExpr, HExprKind};
            use dream_types::{constructor_fn, DefKind};

            let named = |name: &str| -> Type {
                let mut t = method.clone();
                t.text = name.to_string();
                Type::from_token(t).unwrap_or(Type::Unknown)
            };
            let struct_name = params_types
                .first()
                .map(|s| s.trim_end_matches('?').to_string())
                .unwrap_or_default();
            let value = arg_hirs.into_iter().next().flatten();
            let sb_ty = named("StringBuilder");
            let string_ty = named("string");
            let sb_local = self.hir_alloc_local("__json_sb", &sb_ty);
            let ctor = self
                .type_ctx
                .defs
                .lookup(DefKind::Function, &constructor_fn("StringBuilder"));
            let int_ty = self.type_ctx.interner.int();
            let capacity = HExpr::new(int_ty, HExprKind::IntLit(16));
            self.hir_set_new("StringBuilder", ctor, vec![Some(capacity)], &sb_ty);
            let new_sb = self.hir_take();
            if let Some(local) = sb_local {
                self.hir_assign_local_id(local, new_sb);
                let sb_ty_id = self.type_ctx.lower(&sb_ty);
                let sb_read = HExpr::new(sb_ty_id, HExprKind::Var(Binding::Local(local)));
                let write_call = if let Some(adapter) = json_collection_write_fn(&struct_name) {
                    adapter
                } else {
                    method_fn(&struct_name, "write_json")
                };
                self.hir_set_call(&write_call, vec![value, Some(sb_read)], &Type::Void);
                let write_hir = self.hir_take();
                self.hir_expr_stmt(write_hir);
                let sb_read2 = HExpr::new(sb_ty_id, HExprKind::Var(Binding::Local(local)));
                self.hir_set_call(
                    &method_fn("StringBuilder", "build"),
                    vec![Some(sb_read2)],
                    &string_ty,
                );
            } else {
                self.hir_fail();
                self.hir_none();
            }
            return Ok(string_ty);
        }
        if json_op == Some(intrinsics::IntrinsicOp::JsonDeserialize) {
            use dream_hir::{Binding, HExpr, HExprKind};
            use dream_syntax::token::token_kind::TokenKind;

            let named = |name: &str| -> Type {
                let mut t = method.clone();
                t.text = name.to_string();
                Type::from_token(t).unwrap_or(Type::Unknown)
            };
            let t_type = match generic_args.as_ref().and_then(|g| g.first()) {
                Some(t) => Self::monomorphize_type(t, &self.current_generic_bindings),
                None => {
                    diagnostics.report_error(
                        "'Json.deserialize' requires a type argument, e.g. Json.deserialize<T>(text)"
                            .to_string(),
                        Some(method.position),
                    );
                    Type::Void
                }
            };
            let struct_name = t_type.get_type().trim_end_matches('?').to_string();
            let from_json_call = json_collection_de_fn(&struct_name)
                .unwrap_or_else(|| method_fn(&struct_name, "from_json"));
            let text = arg_hirs.into_iter().next().flatten();
            let is_union = self.union_table.contains_key(t_type.get_type().as_str());
            let typed_parser = json_collection_de_fn(&struct_name).is_none() && !is_union;

            let parse_err = named("ParseError");
            let json_value = named("JsonValue");
            let parse_result_ty = Type::Struct(
                synthetic_token(TokenKind::IdentifierToken, "Result"),
                Some(vec![json_value.clone(), parse_err.clone()]),
            );
            let result_ty = Type::Struct(
                synthetic_token(TokenKind::IdentifierToken, "Result"),
                Some(vec![t_type.clone(), parse_err.clone()]),
            );

            let span = method.position;
            self.ensure_union_instantiated(
                "Result",
                &[json_value.clone(), parse_err.clone()],
                &span,
                diagnostics,
            );
            self.ensure_union_instantiated(
                "Result",
                &[t_type.clone(), parse_err.clone()],
                &span,
                diagnostics,
            );

            if typed_parser {
                self.hir_set_call(
                    &method_fn(&struct_name, "from_json_parser_text"),
                    vec![text],
                    &result_ty,
                );
                return Ok(result_ty);
            }

            self.hir_set_call("Json_parse", vec![text], &parse_result_ty);
            let parse_hir = self.hir_take();

            let parse_mangled = parse_result_ty.get_type();
            let result_mangled = result_ty.get_type();
            let parse_info = self.union_table.get(&parse_mangled).cloned();
            let parse_def = self
                .type_ctx
                .defs
                .lookup(dream_types::DefKind::Union, &parse_mangled);
            let result_def = self
                .type_ctx
                .defs
                .lookup(dream_types::DefKind::Union, &result_mangled);

            let (Some(parse_info), Some(parse_def), Some(result_def)) =
                (parse_info, parse_def, result_def)
            else {
                self.hir_fail();
                self.hir_none();
                return Ok(result_ty);
            };
            let (Some(ok_variant), Some(err_variant)) =
                (parse_info.variant("Ok"), parse_info.variant("Err"))
            else {
                self.hir_fail();
                self.hir_none();
                return Ok(result_ty);
            };
            let ok_disc = ok_variant.discriminant as usize;
            let err_disc = err_variant.discriminant as usize;

            let result_temp = self.hir_alloc_local("__json_deser", &result_ty);
            let ok_local = self.hir_alloc_local("__json_ok", &json_value);
            let err_local = self.hir_alloc_local("__json_err", &parse_err);
            let result_ty_id = self.type_ctx.lower(&result_ty);

            let mut ok = parse_hir.is_some()
                && result_temp.is_some()
                && ok_local.is_some()
                && err_local.is_some();

            // Unions with a `@json` derive additionally emit `__json_check_variant` (see
            // `JsonGenerator.expand_union`), which reports an unknown discriminant tag as
            // `Err(ParseError)` instead of `from_json`'s lenient fallback-to-first-variant (kept
            // lenient there so nested/array/tuple/type-param composition never has to thread a
            // `Result` through a constructor-argument expression). Only the top-level
            // `Json.deserialize<T>` entry point gets this strict check.

            // Ok(v) => Result.Ok(T.from_json(v)), or for unions, first validate the variant tag.
            self.hir_open_block();
            if let Some(local) = ok_local {
                let ty_id = self.type_ctx.lower(&json_value);
                let read = HExpr::new(ty_id, HExprKind::Var(Binding::Local(local)));
                if is_union {
                    self.hir_set_call(
                        &method_fn(&struct_name, "__json_check_variant"),
                        vec![Some(read)],
                        &parse_result_ty,
                    );
                    let check_hir = self.hir_take();
                    let inner_ok_local = self.hir_alloc_local("__json_variant_ok", &json_value);
                    let inner_err_local = self.hir_alloc_local("__json_variant_err", &parse_err);
                    if check_hir.is_some() && inner_ok_local.is_some() && inner_err_local.is_some()
                    {
                        self.hir_open_block();
                        if let Some(inner_local) = inner_ok_local {
                            let ty_id = self.type_ctx.lower(&json_value);
                            let read2 =
                                HExpr::new(ty_id, HExprKind::Var(Binding::Local(inner_local)));
                            self.hir_set_call(&from_json_call, vec![Some(read2)], &t_type);
                            let from_json = self.hir_take();
                            self.hir_set_union_new(
                                result_def,
                                ok_disc,
                                vec![from_json],
                                &result_ty,
                            );
                            let wrapped = self.hir_take();
                            self.hir_assign_local_id(
                                result_temp.unwrap_or(dream_hir::LocalId(0)),
                                wrapped,
                            );
                        }
                        let inner_ok_body = self.hir_close_block();
                        let inner_ok_arm = self.hir_variant_arm(
                            parse_def,
                            ok_disc,
                            vec![inner_ok_local.unwrap_or(dream_hir::LocalId(0))],
                            inner_ok_body,
                        );

                        self.hir_open_block();
                        if let Some(inner_local) = inner_err_local {
                            let ty_id = self.type_ctx.lower(&parse_err);
                            let read2 =
                                HExpr::new(ty_id, HExprKind::Var(Binding::Local(inner_local)));
                            self.hir_set_union_new(
                                result_def,
                                err_disc,
                                vec![Some(read2)],
                                &result_ty,
                            );
                            let wrapped = self.hir_take();
                            self.hir_assign_local_id(
                                result_temp.unwrap_or(dream_hir::LocalId(0)),
                                wrapped,
                            );
                        }
                        let inner_err_body = self.hir_close_block();
                        let inner_err_arm = self.hir_variant_arm(
                            parse_def,
                            err_disc,
                            vec![inner_err_local.unwrap_or(dream_hir::LocalId(0))],
                            inner_err_body,
                        );

                        self.hir_switch(check_hir, vec![inner_ok_arm, inner_err_arm], vec![], true);
                    } else {
                        ok = false;
                    }
                } else {
                    self.hir_set_call(&from_json_call, vec![Some(read)], &t_type);
                    let from_json = self.hir_take();
                    self.hir_set_union_new(result_def, ok_disc, vec![from_json], &result_ty);
                    let wrapped = self.hir_take();
                    self.hir_assign_local_id(result_temp.unwrap_or(dream_hir::LocalId(0)), wrapped);
                }
            } else {
                ok = false;
            }
            let ok_body = self.hir_close_block();
            let ok_arm = self.hir_variant_arm(
                parse_def,
                ok_disc,
                vec![ok_local.unwrap_or(dream_hir::LocalId(0))],
                ok_body,
            );

            // Err(e) => Result.Err(e)
            self.hir_open_block();
            if let Some(local) = err_local {
                let ty_id = self.type_ctx.lower(&parse_err);
                let read = HExpr::new(ty_id, HExprKind::Var(Binding::Local(local)));
                self.hir_set_union_new(result_def, err_disc, vec![Some(read)], &result_ty);
                let wrapped = self.hir_take();
                self.hir_assign_local_id(result_temp.unwrap_or(dream_hir::LocalId(0)), wrapped);
            } else {
                ok = false;
            }
            let err_body = self.hir_close_block();
            let err_arm = self.hir_variant_arm(
                parse_def,
                err_disc,
                vec![err_local.unwrap_or(dream_hir::LocalId(0))],
                err_body,
            );

            self.hir_switch(parse_hir, vec![ok_arm, err_arm], vec![], ok);
            if ok {
                self.hir_set_local_read(result_temp.unwrap_or(dream_hir::LocalId(0)), result_ty_id);
            } else {
                self.hir_fail();
                self.hir_none();
            }
            return Ok(result_ty);
        }
        if json_op == Some(intrinsics::IntrinsicOp::JsonFromValue) {
            let t_type = match generic_args.as_ref().and_then(|g| g.first()) {
                Some(t) => Self::monomorphize_type(t, &self.current_generic_bindings),
                None => {
                    diagnostics.report_error(
                        "'Json.from_value' requires a type argument, e.g. Json.from_value<T>(value)"
                            .to_string(),
                        Some(method.position),
                    );
                    Type::Void
                }
            };
            let struct_name = t_type.get_type().trim_end_matches('?').to_string();
            let value = arg_hirs.into_iter().next().flatten();
            let from_json_call = json_collection_de_fn(&struct_name)
                .unwrap_or_else(|| method_fn(&struct_name, "from_json"));
            self.hir_set_call(&from_json_call, vec![value], &t_type);
            return Ok(t_type);
        }

        // Class-level privacy (Axis 1): a non-public generic static method is private to its
        // declaring type, exactly like the non-generic path in `analyze_static_call`. Without
        // this the generic branch below would return early and skip the check entirely.
        if !self.member_accessible(
            template.visibility,
            &template.file_path,
            ctx.parent_function.file_path.as_ref(),
            self.in_methods_of(ctx.parent_function, type_name),
        ) {
            diagnostics.report_error(
                format!("'{}' is private to '{}'", method.text, type_name),
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
            &store_sig.defaults,
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
