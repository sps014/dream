use super::*;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{FunctionNode, Type};
use dream_syntax::token::token_kind::TokenKind;
use dream_text::text_span::TextSpan;

impl<'a> Analyzer<'a> {
    /// Substitutes every generic parameter appearing in a method's parameter or return types
    /// with its concrete type, according to the monomorphization bindings.
    pub(super) fn substitute_generic_signature(
        method: &mut FunctionNode<'a>,
        bindings: &GenericBindings,
    ) {
        for param in &mut method.parameters {
            param.type_ = Self::monomorphize_type(&param.type_, bindings);
        }
        if let Some(ret) = &method.return_type {
            method.return_type = Some(Self::monomorphize_type(ret, bindings));
        }
    }

    pub(in crate::analyzer) fn match_generic_type(
        &self,
        formal: &Type,
        arg: dream_types::TypeId,
        param_name: &str,
    ) -> Option<dream_types::TypeId> {
        use dream_types::TyKind;
        match (formal, self.type_ctx.interner.kind(arg)) {
            (Type::Struct(token, None), _) if token.text == param_name => Some(arg),
            (Type::Generic(name), _) if name == param_name => Some(arg),
            (Type::Array(inner), TyKind::Array(arg_inner)) => {
                self.match_generic_type(inner, *arg_inner, param_name)
            }
            (Type::Tuple(formals), TyKind::Tuple(actuals)) if formals.len() == actuals.len() => {
                formals.iter().zip(actuals).find_map(|(formal, &actual)| {
                    self.match_generic_type(formal, actual, param_name)
                })
            }
            (
                Type::Struct(token, Some(formals)),
                TyKind::Struct(def, actuals)
                | TyKind::Union(def, actuals)
                | TyKind::Interface(def, actuals),
            ) if formals.len() == actuals.len()
                && self
                    .type_ctx
                    .resolve(self.type_ctx.defs.get(*def).kind, &token.text)
                    == Some(*def) =>
            {
                formals.iter().zip(actuals).find_map(|(formal, &actual)| {
                    self.match_generic_type(formal, actual, param_name)
                })
            }
            (Type::Function(formals, ret), TyKind::Func(actuals, arg_ret))
                if formals.len() == actuals.len() =>
            {
                formals
                    .iter()
                    .zip(actuals)
                    .find_map(|(formal, &actual)| {
                        self.match_generic_type(formal, actual, param_name)
                    })
                    .or_else(|| self.match_generic_type(ret, *arg_ret, param_name))
                    .or_else(|| {
                        let Type::Struct(token, Some(args)) = ret.as_ref() else {
                            return None;
                        };
                        let future = self.type_ctx.resolve(DefKind::Struct, FUTURE_TYPE)?;
                        (args.len() == 1
                            && self.type_ctx.resolve(DefKind::Struct, &token.text) == Some(future))
                        .then(|| self.match_generic_type(&args[0], *arg_ret, param_name))
                        .flatten()
                    })
            }
            _ => None,
        }
    }

    /// Determines the concrete type bound to each generic parameter of `template` for one call.
    /// Uses explicit type arguments when given (arity-checked); otherwise infers each parameter
    /// from the actual argument passed to the first formal parameter that is exactly that
    /// parameter. Parameters that cannot be inferred produce a diagnostic.
    pub(super) fn infer_generic_bindings(
        &mut self,
        template: &FunctionNode<'a>,
        generic_args: &Option<Vec<Type>>,
        params_types: &[dream_types::TypeId],
        position: &TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) -> GenericBindings {
        let gen_params = template.generic_parameters.as_deref().unwrap_or(&[]);

        if let Some(generics) = generic_args {
            if !generics.is_empty() {
                Self::check_generic_arity(
                    "function",
                    &template.name.text,
                    gen_params.len(),
                    generics.len(),
                    position,
                    diagnostics,
                );
                return gen_params
                    .iter()
                    .zip(generics.iter())
                    .map(|(param, arg)| {
                        let id = self.type_ctx.lower(arg);
                        (param.text.clone(), self.type_ctx.syntax_type(id))
                    })
                    .collect();
            }
        }

        let scope = self.type_ctx.scope();
        self.type_ctx
            .set_scope(self.graph.module_for_file(template.file_path.as_deref()));
        let bindings = gen_params.iter().map(|param| {
            let concrete = template.parameters.iter().enumerate().find_map(|(i, formal)| {
                params_types.get(i).and_then(|arg| {
                    self.match_generic_type(&formal.type_, *arg, &param.text)
                })
            });
            match concrete {
                Some(concrete) => (param.text.clone(), self.type_ctx.syntax_type(concrete)),
                None => {
                    diagnostics.report_error(
                        format!("Cannot infer generic parameter '{}' of function '{}'; specify type arguments explicitly", param.text, template.name.text),
                        Some(*position),
                    );
                    (param.text.clone(), Type::Unknown)
                }
            }
        }).collect();
        self.type_ctx.set_scope(scope);
        bindings
    }

    /// Returns `ty` with any generic parameter substituted for its concrete type per the
    /// monomorphization bindings, recursing through array wrappers (`T`, `T[]`).
    pub(super) fn monomorphize_type(ty: &Type, bindings: &GenericBindings) -> Type {
        match ty {
            Type::Struct(token, None) => match lookup_binding(bindings, &token.text) {
                Some(concrete) => concrete,
                None => ty.clone(),
            },
            Type::Generic(name) => match lookup_binding(bindings, name) {
                Some(concrete) => concrete,
                None => ty.clone(),
            },
            // A generic struct applied to type arguments (e.g. `List<T>`): substitute inside the
            // arguments so a generic function/method returning `List<T>` resolves to `List<int>`.
            Type::Struct(token, Some(args)) => Type::Struct(
                token.clone(),
                Some(
                    args.iter()
                        .map(|a| Self::monomorphize_type(a, bindings))
                        .collect(),
                ),
            ),
            Type::Array(inner) => Type::Array(Box::new(Self::monomorphize_type(inner, bindings))),
            Type::Tuple(elems) => Type::Tuple(
                elems
                    .iter()
                    .map(|e| Self::monomorphize_type(e, bindings))
                    .collect(),
            ),
            // First-class function types (`fun(T, T): int`) must substitute inside their parameter
            // and return types so a monomorphized callback param (e.g. `sort_by`'s comparator)
            // type-checks against concrete arguments.
            Type::Function(params, ret) => Type::Function(
                params
                    .iter()
                    .map(|p| Self::monomorphize_type(p, bindings))
                    .collect(),
                Box::new(Self::monomorphize_type(ret, bindings)),
            ),
            _ => ty.clone(),
        }
    }

    /// Replaces still-unbound generic parameter names with `Unknown` so a lambda expected type
    /// like `fun(int): TOut` does not pin the return to the placeholder name `TOut`.
    pub(super) fn erase_unbound_generics(
        ty: &Type,
        bindings: &GenericBindings,
        gen_params: &[dream_syntax::token::syntax_token::SyntaxToken],
    ) -> Type {
        let unbound = |name: &str| {
            gen_params.iter().any(|p| p.text == name) && lookup_binding(bindings, name).is_none()
        };
        match ty {
            Type::Struct(token, None) if unbound(&token.text) => Type::Unknown,
            Type::Generic(name) if unbound(name) => Type::Unknown,
            Type::Function(params, ret) => Type::Function(
                params
                    .iter()
                    .map(|p| Self::erase_unbound_generics(p, bindings, gen_params))
                    .collect(),
                Box::new(Self::erase_unbound_generics(ret, bindings, gen_params)),
            ),
            Type::Array(inner) => Type::Array(Box::new(Self::erase_unbound_generics(
                inner, bindings, gen_params,
            ))),
            Type::Tuple(elems) => Type::Tuple(
                elems
                    .iter()
                    .map(|e| Self::erase_unbound_generics(e, bindings, gen_params))
                    .collect(),
            ),
            Type::Struct(token, Some(args)) => Type::Struct(
                token.clone(),
                Some(
                    args.iter()
                        .map(|a| Self::erase_unbound_generics(a, bindings, gen_params))
                        .collect(),
                ),
            ),
            _ => ty.clone(),
        }
    }

    /// Verifies that each concrete type bound by `bindings` satisfies its declared generic
    /// `constraints` (`T : Comparable<T>` etc.), reporting a clear error otherwise. Each bound is
    /// substituted with the same bindings so `Comparable<T>` becomes `Comparable<int>` before the
    /// `implements` lookup; the concrete argument must implement that (mangled) interface.
    pub(super) fn verify_generic_constraints(
        &mut self,
        constraints: &[dream_syntax::nodes::GenericConstraint],
        bindings: &GenericBindings,
        position: &TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        for constraint in constraints {
            let Some(concrete) = bindings.get(&constraint.param.text) else {
                continue;
            };
            for bound in &constraint.bounds {
                if !self.type_satisfies_bound(concrete, bound, bindings, diagnostics) {
                    diagnostics.report_error(
                        format!(
                            "type '{}' does not satisfy the constraint '{}' on generic parameter '{}' (it does not implement that interface)",
                            self.ty_display(concrete),
                            self.ty_display(bound),
                            constraint.param.text
                        ),
                        Some(*position),
                    );
                }
            }
            for kind in &constraint.kinds {
                if !self.type_satisfies_kind(concrete, *kind) {
                    let (want, why) = match kind {
                        dream_syntax::nodes::ConstraintKind::Struct => {
                            ("struct", "it is not a value type")
                        }
                        dream_syntax::nodes::ConstraintKind::Unmanaged => (
                            "unmanaged",
                            "it is not a blittable value type (it contains reference-typed fields, or is a reference type)",
                        ),
                        dream_syntax::nodes::ConstraintKind::Shared => (
                            "shared",
                            "it is not blittable, string, a struct of shared fields, or a 'shared class'",
                        ),
                        dream_syntax::nodes::ConstraintKind::Class => {
                            ("class", "it is not a reference type")
                        }
                    };
                    diagnostics.report_error(
                        format!(
                            "type '{}' does not satisfy the '{}' constraint on generic parameter '{}' ({})",
                            self.ty_display(concrete),
                            want,
                            constraint.param.text,
                            why
                        ),
                        Some(*position),
                    );
                }
            }
        }
    }

    pub(super) fn type_satisfies_kind(
        &mut self,
        concrete: &Type,
        kind: dream_syntax::nodes::ConstraintKind,
    ) -> bool {
        let ty = self.type_ctx.lower(concrete);
        self.type_id_satisfies_kind(ty, kind, &mut indexmap::IndexSet::new())
    }

    pub(super) fn type_id_satisfies_kind(
        &self,
        ty: dream_types::TypeId,
        kind: dream_syntax::nodes::ConstraintKind,
        seen: &mut indexmap::IndexSet<dream_types::TypeId>,
    ) -> bool {
        use dream_syntax::nodes::ConstraintKind;
        use dream_types::{PrimTy, TyKind};
        match self.type_ctx.interner.kind(ty) {
            TyKind::Error => true,
            TyKind::Prim(PrimTy::String) => {
                matches!(kind, ConstraintKind::Class | ConstraintKind::Shared)
            }
            TyKind::Prim(_) => !matches!(kind, ConstraintKind::Class),
            TyKind::Func(_, _) | TyKind::Void => matches!(kind, ConstraintKind::Shared),
            TyKind::Tuple(elems) => {
                !matches!(kind, ConstraintKind::Class)
                    && elems
                        .iter()
                        .all(|&elem| self.type_id_satisfies_kind(elem, kind, seen))
            }
            TyKind::Struct(def, _) => {
                let value = self.type_ctx.defs.is_value(*def);
                match kind {
                    ConstraintKind::Class => !value,
                    ConstraintKind::Struct => value,
                    ConstraintKind::Shared if !value => self.type_ctx.interner.is_shared_def(*def),
                    ConstraintKind::Unmanaged | ConstraintKind::Shared => {
                        if !value {
                            return false;
                        }
                        if !seen.insert(ty) {
                            return true;
                        }
                        self.struct_info(ty).is_some_and(|info| {
                            info.fields
                                .values()
                                .all(|field| self.type_id_satisfies_kind(field.ty, kind, seen))
                        })
                    }
                }
            }
            TyKind::Enum(_) => !matches!(kind, ConstraintKind::Class),
            TyKind::Array(_)
            | TyKind::Object
            | TyKind::Js
            | TyKind::Interface(_, _)
            | TyKind::Union(_, _) => matches!(kind, ConstraintKind::Class),
        }
    }

    pub(super) fn is_unresolved_generic_type(&mut self, ty: &Type) -> bool {
        let id = self.type_ctx.lower(ty);
        matches!(self.type_ctx.interner.kind(id), dream_types::TyKind::Error)
    }

    /// Reports an error unless `ty` satisfies the `unmanaged` (blittable) kind. Used by the raw
    /// byte-blit intrinsics (`Bytes.of`/`Bytes.to`), whose generic bound is verified here rather
    /// than through the normal call-site constraint path (which they bypass).
    pub(super) fn require_unmanaged(
        &mut self,
        ty: &Type,
        who: &str,
        position: &TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        if !self.type_satisfies_kind(ty, dream_syntax::nodes::ConstraintKind::Unmanaged) {
            diagnostics.report_error(
                format!(
                    "'{}' requires an unmanaged (blittable) type, but '{}' is not (it is a reference type, or contains reference-typed fields)",
                    who,
                    self.ty_display(ty)
                ),
                Some(*position),
            );
        }
    }

    /// Like [`Self::require_unmanaged`], but also accepts `T[]` when its element type is itself
    /// unmanaged: the wire/byte-blit intrinsics (`Bytes.of`/`to`, `Bytes.toWire`/`fromWire`) copy
    /// such an array's raw element bytes (a dynamic-length `memory.copy`, never the array pointer
    /// itself), which is exactly as safe as blitting a single blittable value — no reference,
    /// aliasing, or refcounting is involved. Only one level of array is allowed: an array of arrays
    /// is never blittable, since its element type is itself a reference.
    pub(super) fn require_unmanaged_or_array(
        &mut self,
        ty: &Type,
        who: &str,
        position: &TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        if let Type::Array(inner) = ty {
            if self.type_satisfies_kind(inner, dream_syntax::nodes::ConstraintKind::Unmanaged) {
                return;
            }
        }
        self.require_unmanaged(ty, who, position, diagnostics);
    }

    /// True when `concrete` implements the interface named by `bound` (after substituting the
    /// monomorphization `bindings` into `bound`, e.g. `Comparable<T>` -> `Comparable<int>`).
    pub(super) fn type_satisfies_bound(
        &mut self,
        concrete: &Type,
        bound: &Type,
        bindings: &GenericBindings,
        diagnostics: &mut DiagnosticBag,
    ) -> bool {
        let concrete_id = self.type_ctx.lower(concrete);
        if matches!(
            self.type_ctx.interner.kind(concrete_id),
            dream_types::TyKind::Error
        ) {
            return true;
        }
        let bound = substitute_generic_type(bound, bindings);
        let Some((base, args)) = Self::resolve_struct_parts(&bound) else {
            return false;
        };
        self.ensure_interface_instantiated(
            &base,
            &args,
            &bound
                .get_span()
                .unwrap_or_else(|| synthetic_token(TokenKind::IdentifierToken, &base).position),
            diagnostics,
        );
        let iface = self.type_ctx.lower(&bound);
        self.implements_as_interface_ref(concrete_id, iface, diagnostics)
    }
}
