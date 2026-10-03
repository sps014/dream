use super::*;

impl<'a> Analyzer<'a> {
    pub(super) fn analyze_json_intrinsic(
        &mut self,
        call: &GenericStaticMethodCall<'a, '_>,
        params_types: Vec<String>,
        arg_hirs: Vec<Option<dream_hir::HExpr>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        let GenericStaticMethodCall {
            template,
            method,
            generic_args,
            ..
        } = *call;
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
            if struct_name == "JsonValue" {
                self.hir_set_call(
                    &method_fn("Json", "_stringify"),
                    vec![value],
                    &named("string"),
                );
                return Ok(named("string"));
            }
            let sb_ty = named("StringBuilder");
            let string_ty = named("string");
            let sb_local = self.hir_alloc_local("__json_sb", &sb_ty);
            let ctor = self
                .type_ctx
                .resolve(DefKind::Function, &constructor_fn("StringBuilder"));
            let int_ty = self.type_ctx.interner.int();
            let capacity = HExpr::new(int_ty, HExprKind::IntLit(256));
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
                // Diagnose from the *type*, not from whether the generator has already emitted a
                // writer. LSP analysis skips generators, so `Map<string, string>` has no adapter
                // DefId even though a real compile will produce one.
                if let Some(arg) = value.as_ref() {
                    if !self.json_type_encodable(arg.ty) {
                        diagnostics.report_error(
                            format!(
                                "'{}' cannot be serialized to JSON: it has no compile-time JSON encoding. Use 'JsonValue' for mixed or unknown data, or mark a named type '@json'",
                                self.ty_str_display(&struct_name),
                            ),
                            Some(method.position),
                        );
                        self.hir_fail();
                        self.hir_none();
                        return Ok(string_ty);
                    }
                }
                let prim_writer =
                    value
                        .as_ref()
                        .and_then(|arg| match self.type_ctx.interner.kind(arg.ty) {
                            dream_types::TyKind::Prim(p) => Some(match p {
                                dream_types::PrimTy::String => "write_string",
                                dream_types::PrimTy::Bool => "write_bool",
                                dream_types::PrimTy::Int => "write_int",
                                dream_types::PrimTy::Double => "write_number",
                                _ => "",
                            }),
                            _ => None,
                        });
                match prim_writer {
                    Some("") => {
                        diagnostics.report_error(
                            format!(
                                "'{}' cannot be the top-level type of 'Json.serialize': use int, double, bool, or string, or wrap it in a '@json' class",
                                self.ty_str_display(&struct_name),
                            ),
                            Some(method.position),
                        );
                        self.hir_fail();
                        self.hir_none();
                        return Ok(string_ty);
                    }
                    Some(writer) => {
                        self.hir_set_call(
                            &method_fn("Json", writer),
                            vec![Some(sb_read), value],
                            &Type::Void,
                        );
                    }
                    None => {
                        self.ensure_json_callee(&write_call);
                        self.hir_set_call(&write_call, vec![value, Some(sb_read)], &Type::Void);
                    }
                }
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
            let is_union = self.union_info(t_type.get_type().as_str()).is_some();
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

            let t_ty_id = self.type_ctx.lower(&t_type);
            if struct_name != "JsonValue" && !self.json_type_encodable(t_ty_id) {
                diagnostics.report_error(
                    format!(
                        "'{}' cannot be deserialized from JSON: it has no compile-time JSON encoding. Use 'JsonValue' for mixed or unknown data, or mark a named type '@json'",
                        self.ty_display(&t_type),
                    ),
                    Some(method.position),
                );
                self.hir_fail();
                self.hir_none();
                return Ok(result_ty);
            }

            if struct_name == "JsonValue" {
                self.hir_set_call(&method_fn("Json", "_parse"), vec![text], &result_ty);
                return Ok(result_ty);
            }

            // Only named `@json` types carry a generated top-level parser; a bare primitive is
            // encodable as a field but has no `from_json_parser_text` of its own.
            if typed_parser
                && !matches!(
                    self.type_ctx.interner.kind(t_ty_id),
                    dream_types::TyKind::Struct(..)
                )
            {
                diagnostics.report_error(
                    format!(
                        "'{}' cannot be the top-level type of 'Json.deserialize': wrap it in a '@json' class, or deserialize 'JsonValue' and read the value",
                        self.ty_display(&t_type),
                    ),
                    Some(method.position),
                );
                self.hir_fail();
                self.hir_none();
                return Ok(result_ty);
            }

            if typed_parser {
                let parser = method_fn(&struct_name, "from_json_parser_text");
                self.ensure_json_callee(&parser);
                self.hir_set_call(&parser, vec![text], &result_ty);
                return Ok(result_ty);
            }

            self.ensure_json_callee(&from_json_call);
            if is_union {
                self.ensure_json_callee(&method_fn(&struct_name, "__json_check_variant"));
            }
            self.hir_set_call(&method_fn("Json", "_parse"), vec![text], &parse_result_ty);
            let parse_hir = self.hir_take();

            let parse_mangled = parse_result_ty.get_type();
            let result_mangled = result_ty.get_type();
            let parse_info = self.union_info(&parse_mangled).cloned();
            let parse_def = self
                .type_ctx
                .resolve(dream_types::DefKind::Union, &parse_mangled);
            let result_def = self
                .type_ctx
                .resolve(dream_types::DefKind::Union, &result_mangled);

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
            if struct_name == "JsonValue" {
                self.hir_set_last(value);
                return Ok(t_type);
            }
            let t_ty_id = self.type_ctx.lower(&t_type);
            if !self.json_type_encodable(t_ty_id) {
                diagnostics.report_error(
                    format!(
                        "'{}' cannot be converted from JSON: it has no compile-time JSON encoding. Use 'JsonValue' for mixed or unknown data, or mark a named type '@json'",
                        self.ty_display(&t_type),
                    ),
                    Some(method.position),
                );
                self.hir_fail();
                self.hir_none();
                return Ok(t_type);
            }
            let from_json_call = json_collection_de_fn(&struct_name)
                .unwrap_or_else(|| method_fn(&struct_name, "from_json"));
            self.ensure_json_callee(&from_json_call);
            self.hir_set_call(&from_json_call, vec![value], &t_type);
            return Ok(t_type);
        }

        // Class-level privacy (Axis 1): a non-public generic static method is private to its
        // declaring type, exactly like the non-generic path in `analyze_static_call`. Without
        // this the generic branch below would return early and skip the check entirely.
        crate::internal_error!("unclassified json intrinsic")
    }
}
