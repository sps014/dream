//! Type-checking `Enum.Variant(args)` / unit `Enum.Variant` construction, for both concrete unions
//! and generic ones (where the concrete type arguments must be resolved from an expected type or
//! inferred from the constructor arguments before the instance can be monomorphized).

use super::*;
use crate::errors::SemanticError;
use crate::symbol_table::SymbolTable;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{ExpressionNode, FunctionNode, Type};
use dream_syntax::token::syntax_token::SyntaxToken;
use indexmap::IndexMap as HashMap;
use std::cell::RefCell;
use std::rc::Rc;

impl<'a> Analyzer<'a> {
    /// If `enum_name` denotes a discriminated union (concrete or generic) and `variant` names one
    /// of its variants, type-checks the construction `Enum.Variant(args)` and returns its type.
    /// Returns `Ok(None)` when `enum_name` is not a union (so the caller can fall through to its
    /// normal handling, e.g. C-style enum member access or a static method call).
    pub(in crate::analyzer) fn analyze_variant_construction(
        &mut self,
        enum_name: &str,
        variant: &SyntaxToken,
        args: &[ExpressionNode<'a>],
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<Type>, SemanticError> {
        let Some(def) = self.type_ctx.resolve(DefKind::Union, enum_name) else {
            return Ok(None);
        };
        let nominal = self.type_ctx.instantiate(def, vec![]);
        let is_generic = self.generic_union(def).is_some();
        let is_concrete = self.union_info(nominal).is_some();
        if !is_generic && !is_concrete {
            return Ok(None);
        }

        // File/module-level visibility (Axis 2): a non-public enum is only referenceable from its
        // declaring file.
        self.check_type_visible(
            enum_name,
            parent_function.file_path.as_ref(),
            variant.position,
            diagnostics,
        );

        // Declared payload names + types (templated for generic unions). Names reorder
        // `Variant(field: expr)` to positional order before the args are typed.
        let (field_names, field_types): (Vec<String>, Vec<Type>) =
            if let Some(&template) = self.generic_union(def) {
                match template
                    .variants
                    .iter()
                    .find(|v| v.name.text == variant.text)
                {
                    Some(v) => (
                        v.fields.iter().map(|f| f.name.text.clone()).collect(),
                        v.fields.iter().map(|f| f.field_type.clone()).collect(),
                    ),
                    None => {
                        return Err(report(
                            diagnostics,
                            format!("Enum '{}' has no variant '{}'", enum_name, variant.text),
                            Some(variant.position),
                        ));
                    }
                }
            } else {
                let info = match self.union_info(nominal) {
                    Some(info) => info,
                    None => {
                        return Err(report(
                            diagnostics,
                            format!("Enum '{}' could not be resolved", enum_name),
                            Some(variant.position),
                        ));
                    }
                };
                match info.variant(&variant.text) {
                    Some(v) => (
                        v.fields.iter().map(|f| f.name.clone()).collect(),
                        v.fields.iter().map(|f| f.type_.clone()).collect(),
                    ),
                    None => {
                        return Err(report(
                            diagnostics,
                            format!("Enum '{}' has no variant '{}'", enum_name, variant.text),
                            Some(variant.position),
                        ));
                    }
                }
            };

        let defaults: Vec<Option<Type>> = field_names.iter().map(|_| None).collect();
        let args = self.normalize_named_arguments(
            &field_names,
            &defaults,
            args,
            variant.position,
            diagnostics,
            false,
        )?;

        let mut arg_types = Vec::new();
        let mut arg_hirs = Vec::new();
        for arg in &args {
            let t = self.analyze_expression(arg, parent_function, symbol_table, diagnostics)?;
            arg_hirs.push(self.hir_take());
            arg_types.push(t);
        }

        if !is_generic {
            self.validate_variant_payload(
                enum_name,
                &variant.text,
                &field_types,
                &arg_types,
                variant.position,
                diagnostics,
            );
            let result_ty = self.type_ctx.syntax_type(nominal);
            // Construct the union value: resolve its `DefId` and the variant's discriminant.
            let def = self
                .type_ctx
                .resolve(dream_types::DefKind::Union, enum_name);
            let disc = self
                .union_info(nominal)
                .and_then(|i| i.variant(&variant.text))
                .map(|v| v.discriminant as usize);
            match (def, disc) {
                (Some(def), Some(disc)) => self.hir_set_union_new(def, disc, arg_hirs, &result_ty),
                _ => self.hir_none(),
            }
            return Ok(Some(result_ty));
        }

        // Generic union: resolve the concrete type arguments, preferring an explicit expected type
        // (e.g. a `let`/`return` annotation) and otherwise inferring from the arguments.
        let template = *self.generic_union(def).unwrap_or_else(|| {
            crate::internal_error!(
                "generic union '{}' reached generic-instantiation analysis without a registered template",
                enum_name
            )
        });
        let params: Vec<String> = template
            .generic_parameters
            .as_ref()
            .map(|ps| ps.iter().map(|p| p.text.clone()).collect())
            .unwrap_or_default();

        let mut concrete_args: Option<Vec<Type>> = None;
        if let Some(Type::Struct(b, Some(eargs))) = &self.current_expected_type
            && self.type_ctx.resolve(DefKind::Union, &b.text) == Some(def)
            && eargs.len() == params.len()
        {
            concrete_args = Some(eargs.clone());
        }
        if concrete_args.is_none() {
            let arg_ids: Vec<_> = arg_types
                .iter()
                .map(|arg| self.type_ctx.lower(arg))
                .collect();
            let scope = self.type_ctx.scope();
            self.type_ctx.set_scope(def.module);
            let mut binding: HashMap<String, Type> = HashMap::new();
            for param in &params {
                if let Some(id) = field_types
                    .iter()
                    .zip(&arg_ids)
                    .find_map(|(formal, &actual)| self.match_generic_type(formal, actual, param))
                {
                    binding.insert(param.clone(), self.type_ctx.syntax_type(id));
                }
            }
            self.type_ctx.set_scope(scope);
            let resolved: Vec<_> = params
                .iter()
                .filter_map(|param| binding.get(param).cloned())
                .collect();
            if resolved.len() == params.len() {
                concrete_args = Some(resolved);
            }
        }

        let concrete_args = match concrete_args {
            Some(a) => a,
            None => {
                return Err(report(
                    diagnostics,
                    format!(
                        "Cannot infer type arguments for '{}.{}'; add a type annotation (e.g. `let x: {}<...> = ...`)",
                        enum_name, variant.text, enum_name
                    ),
                    Some(variant.position),
                ));
            }
        };

        let bindings = generic_bindings(
            template.generic_parameters.as_deref().unwrap_or(&[]),
            &concrete_args,
        );
        let scope = self.type_ctx.scope();
        self.type_ctx.set_scope(def.module);
        let expected_fields: Vec<Type> = field_types
            .iter()
            .map(|field| {
                let ty = self
                    .type_ctx
                    .lower(&substitute_generic_type(field, &bindings));
                self.type_ctx.syntax_type(ty)
            })
            .collect();
        self.type_ctx.set_scope(scope);
        self.validate_variant_payload(
            enum_name,
            &variant.text,
            &expected_fields,
            &arg_types,
            variant.position,
            diagnostics,
        );

        self.ensure_union_instantiated(enum_name, &concrete_args, &variant.position, diagnostics);
        let ids = concrete_args
            .iter()
            .map(|arg| self.type_ctx.lower(arg))
            .collect();
        let instance = self.type_ctx.instantiate(def, ids);
        let result_ty = self.type_ctx.syntax_type(instance);
        let def = self
            .type_ctx
            .resolve(dream_types::DefKind::Union, enum_name);
        let disc = self
            .union_info(instance)
            .and_then(|i| i.variant(&variant.text))
            .map(|v| v.discriminant as usize);
        match (def, disc) {
            (Some(def), Some(disc)) => self.hir_set_union_new(def, disc, arg_hirs, &result_ty),
            _ => self.hir_none(),
        }
        Ok(Some(result_ty))
    }
}
