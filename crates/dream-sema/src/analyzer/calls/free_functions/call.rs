//! The main free-function call path: dispatches indirect (function-value) calls, constructor calls,
//! generic monomorphization, and overload/arity resolution, then emits the resolved direct call.
//! Also hosts `substitute_default_args`, shared with the constructor and instance-call paths.

use super::*;
use dream_syntax::nodes::ExpressionNode;
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_syntax::token::token_kind::TokenKind;

mod defaults;
mod indirect;
impl<'a> Analyzer<'a> {
    pub(crate) fn analyze_function_call(
        &mut self,
        name: &SyntaxToken,
        generic_args: &Option<Vec<Type>>,
        params: &Vec<ExpressionNode<'a>>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        let mut function_name = name.text.clone();
        let nominal_def = self
            .type_ctx
            .nominal_kind(&function_name)
            .and_then(|kind| self.type_ctx.resolve(kind, &function_name));
        let nominal_type = self
            .type_ctx
            .lower(&Type::Struct(name.clone(), generic_args.clone()));
        let ctor_member = dream_syntax::nodes::types::CONSTRUCTOR_NAME;
        let mut params_types = vec![];
        let mut arg_hirs = vec![];
        // A named argument (`f(x, y: 2)`) must be reordered to its positional slot, and a
        // non-overloaded variadic call's trailing loose arguments collected into an array, *before*
        // any argument is analyzed below (analysis is index-driven). Overloaded positional
        // variadic calls stay unpacked until after overload selection (see
        // `pack_variadic_analyzed_args`).
        let has_named_arg = params
            .iter()
            .any(|a| matches!(a, ExpressionNode::NamedArg(..)));
        let is_ctor_call = self.function_info(&function_name).is_err()
            && generic_args.is_none()
            && self.struct_info(nominal_type).is_some();
        let ctor_key = format!("{function_name}.constructor");
        let callee_signature = |analyzer: &Self| -> Option<(Vec<String>, Vec<Option<Type>>, bool)> {
            if let Ok(info) = analyzer.function_info(&function_name) {
                Some((info.param_names, info.defaults, info.is_variadic))
            } else if is_ctor_call {
                analyzer
                    .method_info(nominal_type, ctor_member)
                    .ok()
                    .map(|info| {
                        (
                            info.param_names.iter().skip(1).cloned().collect(),
                            info.defaults.iter().skip(1).cloned().collect(),
                            info.is_variadic,
                        )
                    })
            } else {
                None
            }
        };
        let normalized_params: Vec<ExpressionNode<'a>>;
        let params: &[ExpressionNode<'a>] = if has_named_arg {
            if is_ctor_call && self.method_overloaded(nominal_type, ctor_member) {
                normalized_params = self.normalize_named_for_candidates(
                    &ctor_key,
                    self.function_table
                        .method_candidates(nominal_type, ctor_member),
                    params,
                    name.position,
                    1,
                    diagnostics,
                )?;
            } else if self.function_overloaded(&function_name) {
                normalized_params = self.normalize_named_for_overloads(
                    &function_name,
                    params,
                    name.position,
                    0,
                    diagnostics,
                )?;
            } else {
                let Some((param_names, defaults, is_variadic)) = callee_signature(self) else {
                    return Err(report(
                        diagnostics,
                        format!("named arguments are not supported for '{}'", function_name),
                        Some(name.position),
                    ));
                };
                normalized_params = self.normalize_named_arguments(
                    &param_names,
                    &defaults,
                    params,
                    name.position,
                    diagnostics,
                    is_variadic,
                )?;
            }
            &normalized_params
        } else if !(self.function_overloaded(&function_name)
            || (is_ctor_call && self.method_overloaded(nominal_type, ctor_member)))
        {
            if let Some((param_names, _, true)) = callee_signature(self) {
                normalized_params = self.collect_variadic_args(param_names.len(), params);
                &normalized_params
            } else {
                params.as_slice()
            }
        } else {
            params.as_slice()
        };

        // `Some(7)` / `Ok(x)` when the expected type is a union: same path as `Option.Some(7)`.
        if self.function_info(&function_name).is_err()
            && !self.function_overloaded(&function_name)
            && self.generic_function_template(&function_name).is_none()
            && generic_args.as_ref().is_none_or(|g| g.is_empty())
            && let Some(expected) = self.current_expected_type.clone() {
                let enum_name = match &expected {
                    Type::Struct(tok, _) => tok.text.clone(),
                    other => other.get_type(),
                };
                let expected_id = self.type_ctx.lower(&expected);
                let variant_known = self
                    .type_ctx
                    .resolve(DefKind::Union, &enum_name)
                    .and_then(|def| self.generic_union(def))
                    .map(|t| t.variants.iter().any(|v| v.name.text == name.text))
                    .or_else(|| {
                        self.union_info(expected_id)
                            .map(|info| info.variant(&name.text).is_some())
                    })
                    .unwrap_or(false);
                if variant_known
                    && let Some(t) = self.analyze_variant_construction(
                        &enum_name,
                        name,
                        params,
                        parent_function,
                        symbol_table,
                        diagnostics,
                    )? {
                        return Ok(t);
                    }
            }

        // When the callee is an unambiguous (non-overloaded) free function, publish each parameter's
        // declared type as the expected type while analyzing the matching argument, so untyped
        // literals such as an empty array `[]` infer their element type from the signature. A plain
        // (non-generic) constructor call gets the same treatment via its `constructor`'s parameter
        // types, so e.g. an `Option<T>`-typed field's `None`/`Some(...)` argument can infer `T`
        // without an explicit annotation.
        let expected_params: Option<Vec<Type>> = if self.function_overloaded(&function_name) {
            None
        } else if let Ok(info) = self.function_info(&function_name) {
            Some(Self::expected_param_types(&info))
        } else if generic_args.is_none()
            && self.struct_info(nominal_type).is_some()
            && !self.method_overloaded(nominal_type, ctor_member)
        {
            self.method_info(nominal_type, ctor_member)
                .ok()
                .map(|info| {
                    Self::expected_param_types(&info)
                        .into_iter()
                        .skip(1)
                        .collect()
                })
        } else if let Some(args) = generic_args.as_ref().filter(|a| !a.is_empty()) {
            // A generic constructor call (`Cell<int>(v)`). The struct isn't
            // instantiated yet at this point (that happens inside `analyze_constructor_call`,
            // after arguments are analyzed below), so its constructor's parameter types are looked
            // up straight from the *template* AST and substituted by hand here — deliberately not
            // via `ensure_struct_instantiated`, which would fully analyze every method's body
            // (including ones a self-referential generic constructing itself inside its own
            // methods would recurse back into before it's registered).
            let concrete_generic_args: Vec<Type> = args
                .iter()
                .map(|t| Self::monomorphize_type(t, &self.current_generic_bindings))
                .collect();
            nominal_def
                .and_then(|def| self.generic_struct(def))
                .and_then(|template| {
                    let type_params = template.generic_parameters.as_deref().unwrap_or(&[]);
                    if type_params.len() != concrete_generic_args.len() {
                        return None;
                    }
                    // If the template declares more than one `constructor` overload, prefer the one
                    // whose `fun(...)` parameter matches an async vs sync lambda argument (Future-
                    // returning vs plain). Same-arity sync/`Future` pairs are common.
                    let ctors: Vec<_> = template
                        .methods
                        .iter()
                        .filter(|m| m.name.text == dream_syntax::nodes::types::CONSTRUCTOR_NAME)
                        .collect();
                    let ctor = match ctors.len() {
                        0 => return None,
                        1 => ctors[0],
                        _ => {
                            let matching: Vec<_> = ctors
                                .into_iter()
                                .filter(|c| c.parameters.len() == params.len())
                                .collect();
                            Self::prefer_fun_overload_for_args(matching, params)?
                        }
                    };
                    let bindings = generic_bindings(type_params, &concrete_generic_args);
                    Some(
                        ctor.parameters
                            .iter()
                            .map(|p| Self::monomorphize_type(&p.type_, &bindings))
                            .collect(),
                    )
                })
        } else {
            None
        };
        let mut arg_is_ref: Vec<bool> = Vec::with_capacity(params.len());
        let saved_call_target = self.current_call_target_name.take();
        self.current_call_target_name = Some(function_name.clone());
        for (i, param) in params.iter().enumerate() {
            let saved_expected = self.current_expected_type.take();
            self.current_expected_type = expected_params.as_ref().and_then(|ps| ps.get(i).cloned());
            if let ExpressionNode::RefArgument(_, inner) = param {
                arg_is_ref.push(true);
                self.current_expected_type = saved_expected;
                match self.analyze_ref_argument(inner, parent_function, symbol_table, diagnostics) {
                    Some((t, hir)) => {
                        arg_hirs.push(hir);
                        params_types.push(self.type_ctx.lower(&t));
                    }
                    None => {
                        arg_hirs.push(None);
                        params_types.push(self.type_ctx.interner.error());
                    }
                }
                continue;
            }
            arg_is_ref.push(false);
            let t = self.analyze_expression(param, parent_function, symbol_table, diagnostics)?;
            self.current_expected_type = saved_expected;
            arg_hirs.push(self.hir_take());
            params_types.push(self.type_ctx.lower(&t));
        }
        self.current_call_target_name = saved_call_target;
        // Calling a `js`-typed local (`cb(a, b)`) invokes the underlying JS value dynamically.
        let name_sym = (*symbol_table).as_ref().borrow().get_symbol(name);
        if let Ok(sym_ty) = name_sym
            && self.is_js_type(&sym_ty) {
                self.hir_set_var(&name.text);
                let recv = self.hir_take();
                self.desugar_js_invoke(recv, arg_hirs, Some(name.position), diagnostics);
                return Ok(Self::js_type());
            }

        // Default: no call HIR. Only the plain free-function tail below opts back in; every other
        // path (indirect, constructor, generic, async, overload/arity errors) leaves `last` cleared.
        self.hir_none();

        // A local bound to a polymorphic generic function item (`let f = natural_order; f(1, 2)`)
        // is called by instantiating the template from the argument types — rewrite to the
        // template name so the generic monomorphization path below runs.
        if let Ok(Type::GenericFunctionItem(gname)) =
            (*symbol_table).as_ref().borrow().get_symbol(name)
        {
            function_name = gname;
        }

        // Indirect call: if the called name is a local variable of function type, validate the
        // arguments against the function-type signature and return its result type.
        // `fun(ref T)` is encoded as `RefBox<T>` in the parameter list.
        let local_fun = match (*symbol_table).as_ref().borrow().get_symbol(name) {
            Ok(Type::Function(param_types, ret)) => Some((param_types, ret)),
            _ => None,
        };
        if let Some((param_types, ret)) = local_fun {
            // `f()` is a `FunctionCall` node (not an `Identifier` read), so mark the local used
            // here — otherwise only `(f)()` counted as a use via the parenthesized path.
            (*symbol_table).as_ref().borrow_mut().mark_used(&name.text);
            if generic_args
                .as_ref()
                .map(|g| !g.is_empty())
                .unwrap_or(false)
            {
                diagnostics.report_error(
                    format!(
                        "type arguments are not valid on non-generic function value '{}'",
                        name.text
                    ),
                    Some(name.position),
                );
            }
            if param_types.len() != params_types.len() {
                diagnostics.report_error(
                    format!(
                        "function value '{}' expects {} arguments, got {}",
                        name.text,
                        param_types.len(),
                        params_types.len()
                    ),
                    Some(name.position),
                );
                return Ok((*ret).clone());
            }
            let expected_is_ref: Vec<bool> = param_types
                .iter()
                .map(|t| Self::peel_ref_box(t).1)
                .collect();
            self.validate_ref_arguments(
                &format!("function value '{}'", name.text),
                &expected_is_ref,
                &arg_is_ref,
                name.position,
                diagnostics,
            );
            let expected_strs: Vec<dream_types::TypeId> = param_types
                .iter()
                .map(|t| self.type_ctx.lower(&Self::peel_ref_box(t).0))
                .collect();
            self.validate_arguments(
                &format!("function value '{}'", name.text),
                &expected_strs,
                &params_types,
                name.position,
                diagnostics,
            );
            self.hir_set_indirect_call(&name.text, arg_hirs, ret.as_ref());
            return Ok((*ret).clone());
        }

        // Interfaces cannot be instantiated: `Animal()` is an error even though `Animal` names a
        // type, because an interface has no fields/constructor and no concrete runtime layout.
        if self.type_ctx.nominal_kind(&function_name) == Some(dream_types::DefKind::Interface) {
            return Err(report(
                diagnostics,
                format!("cannot instantiate interface '{}'", function_name),
                Some(name.position),
            ));
        }
        if self.is_static_class_name(&function_name) {
            return Err(report(
                diagnostics,
                format!("cannot instantiate static class '{}'", function_name),
                Some(name.position),
            ));
        }

        // Constructor call: `Struct(args)` / `Struct<T>(args)`. Only treated as a constructor
        // when no free function (concrete or generic) shadows the name, so prelude factory
        // functions such as `List<T>()` keep their behaviour.
        if std::env::var("DREAM_TRACE_CTOR").is_ok() {
            eprintln!(
                "[gate] {} fn_exists={} overloaded={} generic_fn={} is_struct={}",
                function_name,
                self.function_info(&function_name).is_ok(),
                self.function_overloaded(&function_name),
                self.generic_function_template(&function_name).is_some(),
                self.struct_info(nominal_type).is_some(),
            );
        }
        if self.function_info(&function_name).is_err()
            && !self.function_overloaded(&function_name)
            && self.generic_function_template(&function_name).is_none()
            && (self.struct_info(nominal_type).is_some()
                || nominal_def.is_some_and(|def| self.generic_struct(def).is_some()))
        {
            // Substitute the enclosing monomorphization's bindings into the type arguments, so a
            // generic construction using a type parameter (`ListIterator<T>(this)` inside a
            // monomorphized `List<string>.iterator`) instantiates the concrete `ListIterator_string`
            // rather than the unsubstituted `ListIterator_T`.
            let concrete_generic_args: Option<Vec<Type>> = generic_args.as_ref().map(|g| {
                g.iter()
                    .map(|t| Self::monomorphize_type(t, &self.current_generic_bindings))
                    .collect()
            });
            if std::env::var("DREAM_TRACE_CTOR").is_ok() {
                eprintln!("[ctor-path] reached for {}", name.text);
            }
            let (t, resolved_ctor_name) = self.analyze_constructor_call(
                name,
                &concrete_generic_args,
                &mut params_types,
                &mut arg_hirs,
                parent_function,
                symbol_table,
                diagnostics,
            )?;
            let concrete_type = self.type_ctx.lower(&t);
            if self.struct_info(concrete_type).is_some() {
                let ctor = resolved_ctor_name.as_ref().map(|key| key.0);
                let ctor_summary = self.ide_summary(&t);
                self.record_ide_ref(
                    name.position,
                    ide::IdeTarget::Constructor {
                        ty: concrete_type,
                        display: self.type_id_display(concrete_type),
                    },
                    ctor_summary,
                );
                self.hir_set_new(&name.text, ctor, arg_hirs, &t);
                if let Some(info) = resolved_ctor_name
                    .as_ref()
                    .and_then(|key| self.function_table.functions.get(key))
                    .cloned()
                {
                    self.note_sink_arg_moves(
                        params,
                        &params_types,
                        &info.is_take,
                        true,
                        diagnostics,
                    );
                }
            }
            return Ok(t);
        }

        // (generic function instantiation is factored into `register_generic_function_instance`.)

        // The template and structured arguments must select the same monomorphization as its body.
        let mut generic_instance: Option<crate::function_table::FunctionIdentity> = None;

        // Monomorphization: bind every generic parameter to a concrete type, then register
        // (once) a specialized signature under the mangled name.
        if self.generic_function_template(&function_name).is_some() {
            let template = match self.generic_function_template(&function_name) {
                Some(template) => template,
                None => {
                    diagnostics.report_error(
                        format!("Generic function '{}' could not be resolved", function_name),
                        Some(name.position),
                    );
                    return Ok(Type::Unknown);
                }
            };
            let bindings = self.infer_generic_bindings(
                template,
                generic_args,
                &params_types,
                &name.position,
                diagnostics,
            );
            // A constrained type parameter (`fun sort<T : Comparable<T>>(...)`) must be satisfied by
            // the concrete argument; report a clear error at the call site otherwise.
            self.verify_generic_constraints(
                &template.generic_constraints,
                &bindings,
                &name.position,
                diagnostics,
            );
            let mangled_name = self.register_generic_function_instance(template, &bindings);
            generic_instance = Some(mangled_name);
        }

        // Overloaded free functions resolve by argument types; non-overloaded names keep the
        // direct single-signature lookup (and its precise per-argument diagnostics below).
        let store_sig = if let Some(identity) = generic_instance.as_ref() {
            self.function_table
                .get_function(identity)
                .map_err(|error| report(diagnostics, error.to_string(), Some(name.position)))?
        } else if self.function_overloaded(&function_name) {
            match self.select_function_overload(&function_name, &params_types) {
                Ok(sig) => sig,
                Err(message) => {
                    return Err(report(diagnostics, message, Some(name.position)));
                }
            }
        } else {
            match self.function_info(&function_name) {
                Ok(sig) => sig,
                Err(e) => {
                    return Err(report_with_code(
                        diagnostics,
                        e.to_string(),
                        Some(name.position),
                        "unresolved-name",
                    ));
                }
            }
        };

        self.pack_variadic_analyzed_args(
            &store_sig,
            &mut params_types,
            &mut arg_hirs,
            &mut arg_is_ref,
            0,
        );

        // File/module-level visibility (Axis 2): a non-public free function is only callable from
        // its own file. Static methods dispatched here (mangled `Type_method`) keep their own
        // class-level check in `analyze_static_call`.
        if !self.visible_across_files(
            &store_sig.declaring_file,
            store_sig.visibility,
            parent_function.file_path.as_ref(),
        ) {
            self.report_not_public(
                "Function",
                &name.text,
                &store_sig.declaring_file,
                name.position,
                diagnostics,
            );
        }

        self.check_unsafe_call(&store_sig, name.position, diagnostics);
        self.check_runtime_call(
            &function_name,
            store_sig.runtime_support,
            name.position,
            diagnostics,
        );
        self.check_c_fun_args(store_sig.identity.0, params, symbol_table, diagnostics);

        self.validate_ref_arguments(
            &format!("function '{}'", function_name),
            &store_sig.is_ref,
            &arg_is_ref,
            name.position,
            diagnostics,
        );

        let required = store_sig.required_params();
        let total = store_sig.parameters.len();
        let given = params_types.len();
        if given < required || given > total {
            let message = if required == total {
                format!(
                    "Function {} has {} params but {} params are given",
                    function_name, total, given
                )
            } else {
                format!(
                    "Function {} expects between {} and {} arguments, got {}",
                    function_name, required, total, given
                )
            };
            diagnostics.report_error(message, Some(name.position));
            return Ok(Type::Unknown);
        }

        // Substitute default values for any omitted trailing parameters. Each default is a constant
        // literal, so analyzing `Literal(default)` produces the same type-string and HIR an explicit
        // literal argument would, and feeds the per-index checks and `hir_set_call` below unchanged.
        self.substitute_default_args(
            (&store_sig.defaults, &store_sig.parameter_types),
            &mut params_types,
            &mut arg_hirs,
            parent_function,
            symbol_table,
            diagnostics,
        )?;

        self.validate_arguments(
            &format!("function '{}'", function_name),
            &store_sig.parameters,
            &params_types,
            name.position,
            diagnostics,
        );

        let ret_type = Self::async_return_type(
            store_sig.is_async,
            Some(self.type_ctx.syntax_type(store_sig.resolved_return)),
        );
        // Emit a resolved direct call. A generic call resolves to the template's base `DefId` plus
        // the monomorphization args (so it targets the emitted instance). Overloaded free functions
        // resolve to the selected overload's emitted name (each is a distinct `DefId`);
        // non-overloaded ones resolve directly by their base name.
        self.hir_set_call_identity(&store_sig.identity, arg_hirs, &ret_type);
        self.note_sink_arg_moves(
            params,
            &params_types,
            &store_sig.is_take,
            false,
            diagnostics,
        );
        let call_summary = self.ide_summary(&ret_type);
        self.record_ide_ref(
            name.position,
            ide::IdeTarget::Callee {
                key: store_sig.identity.clone(),
                label: name.text.clone(),
            },
            call_summary,
        );
        Ok(ret_type)
    }
}

fn unwrap_callee_ident<'a>(expr: &'a ExpressionNode<'a>) -> Option<&'a SyntaxToken> {
    match expr {
        ExpressionNode::Identifier(t) => Some(t),
        ExpressionNode::Parenthesized(_, inner) => unwrap_callee_ident(inner),
        _ => None,
    }
}
