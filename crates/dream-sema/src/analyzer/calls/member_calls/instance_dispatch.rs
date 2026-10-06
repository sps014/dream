//! Ordinary and interface instance-method resolution once the receiver type is known and the
//! static/builtin cases have been ruled out.

use super::super::super::*;
use crate::errors::SemanticError;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::types::mangle_generic;
use dream_syntax::nodes::{ExpressionNode, FunctionNode, Type};
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_types::method_fn;

mod generics;
mod interfaces;
impl<'a> Analyzer<'a> {
    /// Resolves and type-checks an instance method call `obj.method(args)` once the receiver type
    /// (`obj_type`) is known and the builtins/static cases have been ruled out: monomorphizes the
    /// Resolves an ordinary instance method call on a concrete (non-interface) receiver. Instantiates
    /// a generic struct receiver, selects the (possibly overloaded) `{Type}_{method}`, enforces privacy and the
    /// argument arity/types, and returns the call's result type (a `Future<T>` for `async`).
    /// Method-level generics (`obj.method<T>(...)`) are monomorphized on the fly, mirroring static
    /// generic methods.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn analyze_instance_method(
        &mut self,
        obj_type: &Type,
        method: &SyntaxToken,
        generic_args: &Option<Vec<Type>>,
        params: &Vec<ExpressionNode<'a>>,
        ctx: &super::super::super::AnalyzerContext<'a, '_>,
        receiver: Option<dream_hir::HExpr>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        // A generic interface receiver (e.g. `Container<int>`) must be monomorphized before dispatch
        // so its concrete method slots exist, even if no implementing class was instantiated earlier
        // in analysis order.
        if let Some((base, args)) = Self::resolve_struct_parts(obj_type) {
            if !args.is_empty() && self.type_ctx.resolve(DefKind::Interface, &base).is_some_and(|def| self.is_generic_interface(def)) {
                self.ensure_interface_instantiated(&base, &args, &method.position, diagnostics);
            }
        }
        // Interface-typed receiver: package `extend Iface` methods (`Collection_int_to_list`) are
        // ordinary `{iface}_{method}` entries — prefer those over itable dispatch.
        let obj_id = self.type_ctx.lower(obj_type);
        if let Some(iface_name) = self.interface_receiver_name(obj_type) {
            let has_extension = self.method_info(obj_id, &method.text).is_ok()
                || self.function_table.generic_methods.contains_key(&(obj_id, method.text.clone()));
            if !has_extension {
                return self.analyze_interface_method(
                    obj_id,
                    method,
                    params,
                    ctx,
                    receiver,
                    diagnostics,
                );
            }
            // Resolve as an instance method on the interface type name itself.
            return self.analyze_instance_method_resolved(
                &iface_name,
                obj_type,
                method,
                generic_args,
                params,
                ctx,
                receiver,
                diagnostics,
            );
        }

        // Struct receivers are monomorphized to their concrete type name; primitive/`object`
        // receivers (which can carry methods via `extend`) use their canonical type name directly.
        let struct_name = match Self::resolve_struct_parts(obj_type) {
            Some((base_name, generic_args)) => {
                // A generic union receiver (e.g. `Option<int>`) is instantiated through the union
                // path so its extension methods are registered; everything else is a struct.
                self.ensure_type_instantiated(
                    &base_name,
                    &generic_args,
                    &method.position,
                    diagnostics,
                );
                mangle_generic(&base_name, &generic_args)
            }
            None => obj_type.get_type(),
        };

        self.analyze_instance_method_resolved(
            &struct_name,
            obj_type,
            method,
            generic_args,
            params,
            ctx,
            receiver,
            diagnostics,
        )
    }

    /// Core instance-method resolution once `struct_name` (mangled receiver type) is known.
    #[allow(clippy::too_many_arguments)]
    fn analyze_instance_method_resolved(
        &mut self,
        struct_name: &str,
        obj_type: &Type,
        method: &SyntaxToken,
        generic_args: &Option<Vec<Type>>,
        params: &Vec<ExpressionNode<'a>>,
        ctx: &super::super::super::AnalyzerContext<'a, '_>,
        mut receiver: Option<dream_hir::HExpr>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        let mut owner = self.type_ctx.lower(obj_type);
        let mut mangled_name = method_fn(struct_name, &method.text);
        let mut effective_struct = struct_name.to_string();

        // Concrete array: instantiate `extend T[]` so `arr.is_empty()` / query helpers resolve.
        if struct_name.ends_with("[]") {
            self.ensure_array_collection(owner, diagnostics);
        }

        // Concrete class missing the method: try package extensions on implemented interfaces.
        let missing = self.method_info(owner, &method.text).is_err()
            && !self.function_table.generic_methods.contains_key(&(owner, method.text.clone()));
        if missing {
            if let Some(ifaces) = self.implemented_interfaces(owner).cloned() {
                for iface in ifaces {
                    let iface_id = iface;
                    let iface = self.type_id_display(iface_id);
                    let ext = method_fn(&iface, &method.text);
                    if self.method_info(iface_id, &method.text).is_ok()
                        || self.function_table.generic_methods.contains_key(&(iface_id, method.text.clone()))
                    {
                        let iface_ty = self.type_ctx.syntax_type(iface_id);
                        self.hir_set_cast(receiver.take(), &iface_ty);
                        receiver = self.hir_take();
                        mangled_name = ext;
                        effective_struct = iface;
                        owner = iface_id;
                        break;
                    }
                }
            }
        }

        // Method-level generics (`pool.dispatch<TIn, TOut>(...)`): monomorphize before the plain
        // `function_table` path, which only knows the unbound template signature.
        if let Some(&template) = self.function_table.generic_methods.get(&(owner, method.text.clone())).and_then(|def| self.generic_functions.get(def)) {
            return self.analyze_generic_instance_method(
                template,
                &mangled_name,
                &effective_struct,
                method,
                generic_args,
                params,
                ctx,
                receiver,
                diagnostics,
            );
        }

        // Reorder named arguments (`obj.method(x, y: 2)`) to positional and collect a
        // non-overloaded variadic call's trailing arguments into an array before index-driven
        // analysis. Overloaded positional variadic calls stay unpacked until after selection.
        let has_named_arg = params
            .iter()
            .any(|a| matches!(a, ExpressionNode::NamedArg(..)));
        let method_info = self.method_info(owner, &method.text).ok();
        let is_variadic = method_info.as_ref().is_some_and(|info| info.is_variadic);
        let is_overloaded = self.method_overloaded(owner, &method.text);
        let normalized_params: Vec<ExpressionNode<'a>>;
        let params: &[ExpressionNode<'a>] = if has_named_arg {
            if is_overloaded {
                normalized_params = self.normalize_named_for_candidates(
                    &mangled_name,
                    self.function_table.method_candidates(owner, &method.text),
                    params,
                    method.position,
                    1,
                    diagnostics,
                )?;
            } else {
                let Some(info) = method_info.as_ref() else {
                    let owner_name = self.type_id_display(owner);
                    let notes = suggest_methods(&self.function_table, owner, &method.text)
                        .iter()
                        .map(|m| format!("similar method exists: '{}.{}'", owner_name, m))
                        .collect();
                    return Err(report_with_notes(
                        diagnostics,
                        format!(
                            "Type '{}' has no method '{}'",
                            owner_name,
                            method.text
                        ),
                        Some(method.position),
                        notes,
                    ));
                };
                let param_names: Vec<String> = info.param_names.iter().skip(1).cloned().collect();
                let defaults: Vec<Option<Type>> = info.defaults.iter().skip(1).cloned().collect();
                normalized_params = self.normalize_named_arguments(
                    &param_names,
                    &defaults,
                    params,
                    method.position,
                    diagnostics,
                    info.is_variadic,
                )?;
            }
            &normalized_params
        } else if is_variadic && !is_overloaded {
            let Some(info) = method_info.as_ref() else {
                crate::internal_error!("non-overloaded variadic method has no signature");
            };
            let param_names_len = info.param_names.len().saturating_sub(1);
            normalized_params = self.collect_variadic_args(param_names_len, params);
            &normalized_params
        } else {
            params.as_slice()
        };

        // When the method is unambiguous (not overloaded), its declared parameter types are known
        // before the arguments are analyzed, so publish them as each argument's expected type (same
        // treatment as an unambiguous free-function call) — this lets an argument lambda without its
        // own type context (e.g. `nums.sort_by((a: int, b: int) => a - b)`) infer from the `fun(...)`
        // parameter type. An overloaded method's parameter types aren't known until the arguments
        // themselves are typed, so it falls back to no expected-type context (unchanged behavior).
        let expected_params: Option<Vec<Type>> = if is_overloaded {
            self.expected_params_for_candidates(self.function_table.method_candidates(owner, &method.text), params, 1)
        } else {
            method_info
                .as_ref()
                .map(|info| {
                    Self::expected_param_types(info)
                        .into_iter()
                        .skip(1) // implicit `this`
                        .collect()
                })
        };

        let call_target = format!("{}.{}", effective_struct, method.text);
        let saved_call_target = self.current_call_target_name.take();
        self.current_call_target_name = Some(call_target);

        // Analyze the explicit arguments once, then resolve the method (overloaded methods select
        // by argument types, with the receiver supplied as the implicit `this` argument).
        let (mut arg_types, mut arg_hirs, mut arg_is_ref) = self
            .analyze_call_arguments_expecting_ref(
                params,
                expected_params.as_deref(),
                ctx.parent_function,
                ctx.symbol_table,
                diagnostics,
            )?;

        self.current_call_target_name = saved_call_target;

        let store_sig = if is_overloaded {
            let mut selection_args = Vec::with_capacity(arg_types.len() + 1);
            selection_args.push(owner);
            selection_args.extend(arg_types.iter().cloned());
            match self.select_method_overload(owner, &method.text, &selection_args) {
                Ok(sig) => sig,
                Err(message) => {
                    return Err(report(diagnostics, message, Some(method.position)));
                }
            }
        } else {
            match self.method_info(owner, &method.text) {
                Ok(s) => s.clone(),
                Err(_) => {
                    let owner_name = self.type_id_display(owner);
                    let notes = suggest_methods(&self.function_table, owner, &method.text)
                        .iter()
                        .map(|m| format!("similar method exists: '{}.{}'", owner_name, m))
                        .collect();
                    return Err(report_noted(
                        diagnostics,
                        format!(
                            "Type '{}' has no method '{}'",
                            owner_name,
                            method.text
                        ),
                        Some(method.position),
                        notes,
                        Some("missing-member"),
                    ));
                }
            }
        };

        self.pack_variadic_analyzed_args(
            &store_sig,
            &mut arg_types,
            &mut arg_hirs,
            &mut arg_is_ref,
            1,
        );

        // Private methods (the default) may only be called from within the declaring type's own
        // methods; `internal` from anywhere in the same module; `public` exposes them everywhere.
        if !store_sig.visibility.is_public() {
            let base_name = Self::resolve_struct_parts(obj_type)
                .map(|(b, _)| b)
                .unwrap_or_else(|| obj_type.get_type());
            if !self.member_accessible(
                store_sig.visibility,
                &store_sig.declaring_file,
                ctx.parent_function.file_path.as_ref(),
                self.in_methods_of(ctx.parent_function, owner),
            ) {
                diagnostics.report_error(
                    format!(
                        "'{}' is private to '{}'",
                        method.text,
                        self.ty_str_display(&base_name)
                    ),
                    Some(method.position),
                );
            }
        }

        self.check_unsafe_call(&store_sig, method.position, diagnostics);
        self.check_runtime_call(
            &format!("{}.{}", effective_struct, method.text),
            store_sig.runtime_support,
            method.position,
            diagnostics,
        );

        let mut expected_params = store_sig.parameters.clone();
        let mut expected_defaults = store_sig.defaults.clone();
        let mut expected_is_ref = store_sig.is_ref.clone();
        let mut expected_is_take = store_sig.is_take.clone();

        // Remove 'this' from the expected params check since we supply it implicitly
        if !expected_params.is_empty() {
            expected_params.remove(0);
        }
        if !expected_defaults.is_empty() {
            expected_defaults.remove(0);
        }
        if !expected_is_ref.is_empty() {
            expected_is_ref.remove(0);
        }
        if !expected_is_take.is_empty() {
            expected_is_take.remove(0);
        }
        let mut expected_param_tys = store_sig.parameter_types.clone();
        if !expected_param_tys.is_empty() {
            expected_param_tys.remove(0);
        }
        self.validate_ref_arguments(
            &format!("method '{}'", method.text),
            &expected_is_ref,
            &arg_is_ref,
            method.position,
            diagnostics,
        );

        let method_label = format!("{}.{}", self.type_id_display(owner), method.text);
        let total = expected_params.len();
        let required = Self::required_arg_count(&expected_defaults, total);
        let given = arg_types.len();
        if given < required || given > total {
            let message = if required == total {
                format!(
                    "function {} expects {} parameters, got {}",
                    method_label, total, given
                )
            } else {
                format!(
                    "function {} expects between {} and {} parameters, got {}",
                    method_label, required, total, given
                )
            };
            diagnostics.report_error(message, Some(method.position));
            self.hir_none();
            return Ok(Type::Unknown);
        }

        // Fill omitted trailing arguments with their default values before type-checking/emit.
        self.substitute_default_args(
            (&expected_defaults, &expected_param_tys),
            &mut arg_types,
            &mut arg_hirs,
            ctx.parent_function,
            ctx.symbol_table,
            diagnostics,
        )?;

        self.validate_arguments(
            &format!("function {}", method_label),
            &expected_params,
            &arg_types,
            method.position,
            diagnostics,
        );

        // An `async` method yields a `Future<T>` handle (carried by the `MethodCall`); `await`
        // unwraps it.
        let ret_type = Self::async_return_type(store_sig.is_async, Some(self.type_ctx.syntax_type(store_sig.resolved_return)));
        // Overloaded methods each register a distinct `DefId` under their emitted (signature-mangled)
        // name; resolve to the selected overload's name so the call targets the right instance.
        // Non-overloaded methods keep their base-mangled name.
        self.hir_set_method_call(receiver, &store_sig.identity, arg_hirs, &ret_type);
        self.note_sink_arg_moves(params, &arg_types, &expected_is_take, false, diagnostics);
        let call_summary = self.ide_summary(&ret_type);
        self.record_ide_ref(
            method.position,
            ide::IdeTarget::Callee {
                key: store_sig.identity.clone(),
                label: method.text.clone(),
            },
            call_summary,
        );
        Ok(ret_type)
    }

    pub(crate) fn in_methods_of(&self, parent_function: &FunctionNode<'a>, owner: dream_types::TypeId) -> bool {
        let Some(identity) = self.function_table.declaration_node(parent_function) else { return false; };
        self.function_table.methods.iter().any(|((receiver, _), keys)| *receiver == owner && keys.iter().any(|key| key.0 == identity.0))
            || self.function_table.generic_methods.iter().any(|((receiver, _), def)| *receiver == owner && *def == identity.0)
    }
}

/// Case-insensitive close-match suggestions for `did you mean` notes: prefix match or
/// Levenshtein distance <= 2 over the methods of `struct_name` registered in the table.
fn suggest_methods(
    table: &crate::function_table::FunctionTable,
    owner: dream_types::TypeId,
    wanted: &str,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let want = wanted.to_ascii_lowercase();
    for (receiver, member) in table.methods.keys() {
        if *receiver != owner { continue; }
        let m = member.clone();
        let lm = m.to_ascii_lowercase();
        if (lm.starts_with(&want) || levenshtein(&lm, &want) <= 2) && !out.contains(&m) {
            out.push(m);
        }
        if out.len() >= 3 {
            break;
        }
    }
    out.sort();
    out
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j2, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur.push((prev[j2] + cost).min(cur[j2] + 1).min(prev[j2 + 1] + 1));
        }
        prev = cur;
        if i > 40 {
            break;
        }
    }
    *prev.last().unwrap_or(&usize::MAX)
}
