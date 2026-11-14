//! `Type.method(args)` static/namespaced call analysis, split by concern:
//! - this module: the `try_analyze_static_method` dispatch (variant construction, generic-class
//!   static methods, and routing to the generic-static / plain paths).
//! - [`intrinsics`]: on-the-fly monomorphization of generic static methods, including the
//!   `System.print`, `Buffer.alloc`, `Bytes.of`/`to`, and `Promise.*` intrinsics.
//! - [`plain`]: plain (non-generic) static-method resolution (`analyze_static_call`).

use super::super::super::*;
use dream_syntax::nodes::ExpressionNode;

mod class_infer;
mod intrinsics;
mod plain;

impl<'a> Analyzer<'a> {
    fn analyze_static_call_through_parameter(
        &mut self,
        bound: &Type,
        method: &SyntaxToken,
        generic_args: &Option<Vec<Type>>,
        params: &Vec<ExpressionNode<'a>>,
        ctx: &AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        if let Type::Struct(name, Some(args)) = bound
            && self
                .type_ctx
                .nominal_kind(&name.text)
                .and_then(|kind| self.type_ctx.resolve(kind, &name.text))
                .and_then(|def| self.generic_struct(def))
                .is_some()
        {
            self.ensure_struct_instantiated(&name.text, args, &method.position, diagnostics);
        }
        let owner = self.type_ctx.lower(bound);
        let type_name = self.type_id_display(owner);
        if let Some(template) = self.generic_static_template(owner, &method.text, params.len()) {
            let base = format!("{type_name}.{}", method.text);
            return self.analyze_generic_static_method(
                intrinsics::GenericStaticMethodCall {
                    template,
                    base: &base,
                    type_name: &type_name,
                    method,
                    generic_args,
                    params,
                },
                ctx,
                diagnostics,
            );
        }
        if self.method_overloaded(owner, &method.text)
            || self.method_info(owner, &method.text).is_ok()
        {
            return self.analyze_static_call(
                owner,
                method,
                params,
                ctx.parent_function,
                ctx.symbol_table,
                diagnostics,
            );
        }
        Err(report_with_code(
            diagnostics,
            format!(
                "Type '{}' has no static method '{}'",
                type_name, method.text
            ),
            Some(method.position),
            "missing-member",
        ))
    }

    /// The generic static method `owner.name` whose parameter count accepts `argc` arguments,
    /// or the first declared one when none does (its arity diagnostic then explains the call).
    pub(in crate::analyzer) fn generic_static_template(
        &self,
        owner: dream_types::TypeId,
        name: &str,
        argc: usize,
    ) -> Option<&'a FunctionNode<'a>> {
        let defs = self
            .function_table
            .generic_method_overloads
            .get(&(owner, name.to_string()))?;
        let templates = defs
            .iter()
            .filter_map(|def| self.generic_functions.get(def).copied());
        let accepts = |template: &&'a FunctionNode<'a>| {
            let required = template
                .parameters
                .iter()
                .filter(|p| p.default.is_none() && !p.is_variadic)
                .count();
            let variadic = template.parameters.last().is_some_and(|p| p.is_variadic);
            argc >= required && (variadic || argc <= template.parameters.len())
        };
        templates
            .clone()
            .find(accepts)
            .or_else(|| templates.into_iter().next())
    }

    /// Handles `Type.method(args)` static dispatch when the receiver `id` names a type rather than
    /// a local: discriminated-union variant construction, on-the-fly monomorphization of generic
    /// static methods (including the `Buffer.alloc` and promise-combinator intrinsics), and plain
    /// static-method resolution. Returns `Ok(Some(type))` when handled, `Ok(None)` when `id` is a
    /// local or names no type (so the caller falls through to instance dispatch), and reports a
    /// missing static method when `id` names a type without one.
    pub(crate) fn try_analyze_static_method(
        &mut self,
        id: &SyntaxToken,
        method: &SyntaxToken,
        generic_args: &Option<Vec<Type>>,
        params: &Vec<ExpressionNode<'a>>,
        ctx: &AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<Type>, SemanticError> {
        // The receiver names a type (not a local variable), so resolve `{type}_{method}` directly
        // with no implicit `this`.
        let is_local = (*ctx.symbol_table).as_ref().borrow().get_symbol(id).is_ok();
        if is_local {
            return Ok(None);
        }

        // Generic bodies are analyzed once per instantiation, so `T.member(...)` names the
        // concrete type bound to `T` in this instance.
        if let Some(bound) = self.current_generic_bindings.get(&id.text).cloned() {
            return self
                .analyze_static_call_through_parameter(
                    &bound,
                    method,
                    generic_args,
                    params,
                    ctx,
                    diagnostics,
                )
                .map(Some);
        }

        // `Enum.Variant(args)`: construct a discriminated-union value.
        if let Some(t) = self.analyze_variant_construction(
            &id.text,
            method,
            params,
            ctx.parent_function,
            ctx.symbol_table,
            diagnostics,
        )? {
            return Ok(Some(t));
        }

        let type_name = id.text.clone();
        let owner = self.type_ctx.lower(&Self::type_from_name(&type_name));
        let base = format!("{type_name}.{}", method.text);
        let nominal_def = self
            .type_ctx
            .nominal_kind(&type_name)
            .and_then(|kind| self.type_ctx.resolve(kind, &type_name));

        // File/module-level visibility (Axis 2): reaching a static member requires the type itself
        // to be visible. A non-public class/struct is only referenceable from its declaring file.
        if let Some(info) = self.struct_info(owner) {
            if !self.visible_across_files(
                &info.file_path,
                info.visibility,
                ctx.parent_function.file_path.as_ref(),
            ) && !self
                .generator_can_access_type(&type_name, ctx.parent_function.file_path.as_ref())
            {
                let decl_file = info.file_path.clone();
                self.report_not_public("Type", &type_name, &decl_file, id.position, diagnostics);
            }
        } else if let Some(template) = nominal_def.and_then(|def| self.generic_struct(def)) {
            let (decl_file, visibility) = (template.file_path.clone(), template.visibility);
            if !self.visible_across_files(
                &decl_file,
                visibility,
                ctx.parent_function.file_path.as_ref(),
            ) && !self
                .generator_can_access_type(&type_name, ctx.parent_function.file_path.as_ref())
            {
                self.report_not_public("Type", &type_name, &decl_file, id.position, diagnostics);
            }
        } else {
            self.check_type_visible(
                &type_name,
                ctx.parent_function.file_path.as_ref(),
                id.position,
                diagnostics,
            );
        }

        // A static method on a *generic class* (`Cache<int>.make(...)` or inferred `Cache.make(...)`).
        // Explicit type arguments ride on the call's generic-argument slot; otherwise they are
        // inferred from the expected type or the arguments. Monomorphize the class so its concrete
        // static methods (`Cache_int_make`, ...) are registered, then dispatch through the normal
        // static-call path (which enforces class-level privacy).
        if nominal_def
            .and_then(|def| self.generic_struct(def))
            .is_some()
        {
            let args: Vec<Type> = match generic_args {
                Some(a) if !a.is_empty() => a
                    .iter()
                    .map(|t| Self::monomorphize_type(t, &self.current_generic_bindings))
                    .collect(),
                _ => match self.infer_generic_class_static_type_args(
                    &type_name,
                    method,
                    params,
                    ctx,
                    diagnostics,
                ) {
                    Some(inferred) => inferred,
                    None => return Ok(Some(Type::Unknown)),
                },
            };
            self.ensure_struct_instantiated(&type_name, &args, &id.position, diagnostics);
            let concrete = Type::Struct(id.clone(), Some(args));
            let concrete_id = self.type_ctx.lower(&concrete);
            let ret = self.analyze_static_call(
                concrete_id,
                method,
                params,
                ctx.parent_function,
                ctx.symbol_table,
                diagnostics,
            )?;
            return Ok(Some(ret));
        }

        // `Union<T>.method(...)` on a generic union: instantiate it so its template and
        // extension methods attach, then dispatch on the concrete union.
        if let Some(args) = generic_args.as_ref().filter(|a| !a.is_empty())
            && nominal_def.is_some_and(|def| self.generic_union(def).is_some())
        {
            let args: Vec<Type> = args
                .iter()
                .map(|t| Self::monomorphize_type(t, &self.current_generic_bindings))
                .collect();
            self.ensure_union_instantiated(&type_name, &args, &id.position, diagnostics);
            let concrete_id = self.type_ctx.lower(&Type::Struct(id.clone(), Some(args)));
            return Ok(Some(self.analyze_static_call(
                concrete_id,
                method,
                params,
                ctx.parent_function,
                ctx.symbol_table,
                diagnostics,
            )?));
        }

        // Support generic static method calls by monomorphizing them on the fly.
        if let Some(template) = self.generic_static_template(owner, &method.text, params.len()) {
            let t = self.analyze_generic_static_method(
                intrinsics::GenericStaticMethodCall {
                    template,
                    base: &base,
                    type_name: &type_name,
                    method,
                    generic_args,
                    params,
                },
                ctx,
                diagnostics,
            )?;
            return Ok(Some(t));
        }

        if self.method_overloaded(owner, &method.text)
            || self.method_info(owner, &method.text).is_ok()
        {
            return Ok(Some(self.analyze_static_call(
                owner,
                method,
                params,
                ctx.parent_function,
                ctx.symbol_table,
                diagnostics,
            )?));
        }

        // Instance dispatch would analyze the type name as a value and report it as an unknown
        // variable, hiding that only the member is missing.
        if nominal_def.is_some() && self.function_info(&type_name).is_err() {
            return Err(report_with_code(
                diagnostics,
                format!(
                    "Type '{}' has no static method '{}'",
                    type_name, method.text
                ),
                Some(method.position),
                "missing-member",
            ));
        }

        Ok(None)
    }
}
