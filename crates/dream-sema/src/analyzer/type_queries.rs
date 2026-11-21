use super::*;

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn generic_union(
        &self,
        def: dream_types::DefId,
    ) -> Option<&&'a EnumDeclarationNode<'a>> {
        self.generic_unions.get(&def)
    }
    pub(in crate::analyzer) fn implemented_interfaces(
        &self,
        ty: dream_types::TypeId,
    ) -> Option<&Vec<dream_types::TypeId>> {
        self.implements.get(&ty)
    }
    pub(in crate::analyzer) fn interface_method_list(
        &self,
        ty: dream_types::TypeId,
    ) -> Option<&Vec<&'a FunctionNode<'a>>> {
        self.interface_methods.get(&ty)
    }
    pub(in crate::analyzer) fn interface_index(&self, ty: dream_types::TypeId) -> Option<usize> {
        self.interface_methods.get_index_of(&ty)
    }
    pub(in crate::analyzer) fn interface_decl(
        &self,
        def: dream_types::DefId,
    ) -> Option<&&'a dream_syntax::nodes::InterfaceDeclarationNode<'a>> {
        self.interface_decls.get(&def)
    }
    pub(in crate::analyzer) fn generic_interface(
        &self,
        def: dream_types::DefId,
    ) -> Option<&&'a dream_syntax::nodes::InterfaceDeclarationNode<'a>> {
        self.generic_interfaces.get(&def)
    }
    pub(in crate::analyzer) fn enum_members(
        &self,
        def: dream_types::DefId,
    ) -> Option<&IndexMap<String, i32>> {
        self.enum_table.get(&def)
    }
    pub(in crate::analyzer) fn union_info(
        &self,
        ty: dream_types::TypeId,
    ) -> Option<&crate::union_table::UnionInfo> {
        self.union_table.get(&ty)
    }
    pub(in crate::analyzer) fn generic_struct(
        &self,
        def: dream_types::DefId,
    ) -> Option<&&'a dream_syntax::nodes::StructDeclarationNode<'a>> {
        self.generic_structs.get(&def)
    }
    pub(in crate::analyzer) fn struct_info(
        &self,
        ty: dream_types::TypeId,
    ) -> Option<&crate::struct_table::StructInfo> {
        self.struct_table.get_struct(ty)
    }
    /// Builds the `Future<T>` type carrying inner type `inner`. Async-call results are this type,
    /// and `await` unwraps it back to `inner`.
    pub(in crate::analyzer) fn future_type(inner: Type) -> Type {
        Type::Struct(
            synthetic_token(TokenKind::IdentifierToken, FUTURE_TYPE),
            Some(vec![inner]),
        )
    }

    /// Reports the shared "wrong number of type arguments" diagnostic for a generic instantiation
    /// when `expected` and `actual` differ. `kind` is the declaration keyword used in the message
    /// (e.g. "enum" / "class" / "interface" / "function") and `name` the generic base's name.
    pub(in crate::analyzer) fn check_generic_arity(
        kind: &str,
        name: &str,
        expected: usize,
        actual: usize,
        position: &TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        if expected != actual {
            diagnostics.report_error(
                format!(
                    "Generic {} '{}' expects {} type argument(s), but {} were provided",
                    kind, name, expected, actual
                ),
                Some(*position),
            );
        }
    }

    /// The minimum number of arguments a call must supply, given the callee's parallel trailing
    /// `defaults` list and its `total` parameter count: every parameter up to the first one carrying
    /// a default is required. Mirrors `FunctionTableInfo::required_params` for callers that work on a
    /// sliced defaults list (e.g. instance/constructor calls that first drop the implicit `this`).
    pub(in crate::analyzer) fn required_arg_count(
        defaults: &[Option<Type>],
        total: usize,
    ) -> usize {
        defaults.iter().position(|d| d.is_some()).unwrap_or(total)
    }

    /// The result type of a (possibly `async`) call: calling an `async` function/method is eager and
    /// yields a `Future<T>` handle (where `T` is the declared return type, defaulting to `void`),
    /// which an enclosing `await` unwraps back to `T`. Non-async calls yield `T` directly.
    pub(in crate::analyzer) fn async_return_type(
        is_async: bool,
        return_type: Option<Type>,
    ) -> Type {
        let base = return_type.unwrap_or(Type::Void);
        if is_async {
            Self::future_type(base)
        } else {
            base
        }
    }

    /// If `ty` is a `Future<T>`, returns the inner `T`; otherwise `None`.
    pub(in crate::analyzer) fn future_inner_type(ty: &Type) -> Option<Type> {
        match ty {
            Type::Struct(token, Some(args)) if token.text == FUTURE_TYPE && args.len() == 1 => {
                Some(args[0].clone())
            }
            _ => None,
        }
    }

    /// Pretty-prints an AST type for diagnostics via the interned type graph.
    /// Use this (or [`Self::ty_str_display`]) in every user-facing message; [`Type::get_type`]
    /// is the mangled identity spelling (`List_List_Point`) and must not appear in diagnostics.
    pub(in crate::analyzer) fn ty_display(&mut self, ty: &Type) -> String {
        let id = self.type_ctx.lower(ty);
        dream_types::display_name(&self.type_ctx.interner, &self.type_ctx.defs, id)
    }

    /// Pretty-prints a lexical source name for diagnostics.
    pub(in crate::analyzer) fn ty_str_display(&mut self, s: &str) -> String {
        if dream_syntax::nodes::types::is_unknown_type_name(s) {
            return s.to_string();
        }
        let id = self
            .type_ctx
            .resolved_type(s)
            .unwrap_or_else(|| self.type_ctx.interner.error());
        dream_types::display_name(&self.type_ctx.interner, &self.type_ctx.defs, id)
    }

    pub(in crate::analyzer) fn is_static_class_name(&self, name: &str) -> bool {
        self.type_ctx
            .resolve(DefKind::Struct, name)
            .map(|id| self.type_ctx.defs.is_static(id))
            .unwrap_or(false)
    }

    /// Rejects a literal parameter default that the parameter's type cannot hold, so the error is
    /// reported at the declaration rather than at a call that omits the argument. Generic
    /// declarations are checked per instantiation at the call site.
    pub(in crate::analyzer) fn check_parameter_defaults(
        &mut self,
        params: &[dream_syntax::nodes::ParameterNode],
        generic_parameters: &[String],
        diagnostics: &mut DiagnosticBag,
    ) {
        if !generic_parameters.is_empty() {
            return;
        }
        for param in params {
            let Some(default) = &param.default else {
                continue;
            };
            let default = Self::retarget_numeric_literal(default, Some(&param.type_));
            let value = self.type_ctx.lower(&default);
            let is_literal = value == self.type_ctx.interner.string()
                || matches!(
                    self.type_ctx.interner.kind(value),
                    dream_types::TyKind::Prim(_)
                );
            if !is_literal {
                continue;
            }
            let target = self.type_ctx.lower(&param.type_);
            if !self.value_type_assignable(target, value, diagnostics) {
                diagnostics.report_error(
                    format!(
                        "default value of parameter '{}' has type '{}', but the parameter is '{}'",
                        param.name.text,
                        self.type_id_display(value),
                        self.type_id_display(target),
                    ),
                    default.get_span().or(Some(param.name.position)),
                );
            }
        }
    }

    /// Checks that every name in a written type resolves, enforces type visibility, and rejects
    /// static classes in value positions. Returns false when the type was rejected.
    pub(in crate::analyzer) fn check_declared_type(
        &self,
        ty: &Type,
        parameters: &[String],
        diagnostics: &mut DiagnosticBag,
    ) -> bool {
        match ty {
            Type::Struct(token, args) => {
                if args.is_none() && parameters.contains(&token.text) {
                    return true;
                }
                // Storage synthesized by lowering and the `fun(ref T)` grammar is not a source type name.
                let written =
                    token.position.end > token.position.start && token.kind != TokenKind::RefToken;
                // Stdlib sources may name types from packages that this program never loads.
                if written
                    && !diagnostics
                        .file_path
                        .as_deref()
                        .is_some_and(dream_stdlib::is_std_source)
                    && self.type_ctx.resolved_type(&token.text).is_none()
                    && self.type_ctx.nominal_kind(&token.text).is_none()
                {
                    report_noted(
                        diagnostics,
                        format!("unknown type '{}'", token.text),
                        Some(token.position),
                        import_note(&token.text).into_iter().collect(),
                        Some("unresolved-name"),
                    );
                    return false;
                }
                let mut ok = true;
                if written {
                    let caller = diagnostics.file_path.as_deref().map(Arc::<str>::from);
                    self.check_type_visible(
                        &token.text,
                        caller.as_ref(),
                        token.position,
                        diagnostics,
                    );
                }
                if self.is_static_class_name(&token.text) {
                    diagnostics.report_error(
                        format!(
                            "'{}' is a static class and cannot be used as a type",
                            token.text
                        ),
                        Some(token.position),
                    );
                    ok = false;
                }
                if let Some(args) = args {
                    for a in args {
                        ok &= self.check_declared_type(a, parameters, diagnostics);
                    }
                }
                ok
            }
            Type::Array(inner) => self.check_declared_type(inner, parameters, diagnostics),
            Type::Tuple(elems) => {
                let mut ok = true;
                for e in elems {
                    ok &= self.check_declared_type(e, parameters, diagnostics);
                }
                ok
            }
            Type::Function(params, ret) => {
                let mut ok = true;
                for p in params {
                    ok &= self.check_declared_type(p, parameters, diagnostics);
                }
                self.check_declared_type(ret, parameters, diagnostics) && ok
            }
            _ => true,
        }
    }

    /// If `ty` is a struct, returns its base name and the list of concrete generic type
    /// arguments (empty for non-generic structs). Returns `None` for any non-struct type. Does
    /// NOT recurse into arrays (a method/member access on an array is invalid and must surface
    /// as an error).
    pub(in crate::analyzer) fn resolve_struct_parts(ty: &Type) -> Option<(String, Vec<Type>)> {
        match ty {
            Type::Struct(token, args) => {
                Some((token.text.clone(), args.clone().unwrap_or_default()))
            }
            _ => None,
        }
    }

    /// True when `ty` is a C-style integer enum (named `i32` constants). Discriminated unions
    /// share `Type::Struct` spelling but live in `union_table`, not `enum_table`.
    pub(in crate::analyzer) fn is_c_style_enum(&self, ty: &Type) -> bool {
        let Some((base, args)) = Self::resolve_struct_parts(ty) else {
            return false;
        };
        args.is_empty()
            && self
                .type_ctx
                .resolve(DefKind::Enum, &base)
                .is_some_and(|def| self.enum_members(def).is_some())
    }

    /// Reports an `is` test that can never succeed outside generic code: a concrete value tested
    /// against another type, or an interface value tested against a class that does not implement
    /// it. Generic instantiations fold such tests to `false` on purpose.
    pub(in crate::analyzer) fn check_is_possible(
        &mut self,
        value_ty: &Type,
        tested: &Type,
        span: Option<dream_text::text_span::TextSpan>,
        diagnostics: &mut dream_diagnostics::DiagnosticBag,
    ) {
        if !self.current_generic_bindings.is_empty()
            || value_ty.is_unknown()
            || tested.is_unknown()
            || value_ty.is_object()
        {
            return;
        }
        let value_id = self.type_ctx.lower(value_ty);
        let tested_id = self.type_ctx.lower(tested);
        if value_id == tested_id
            || matches!(
                self.type_ctx.interner.kind(tested_id),
                dream_types::TyKind::Error
            )
        {
            return;
        }
        let impossible = if self.is_interface_name(value_id) {
            !self.is_interface_name(tested_id)
                && !tested.is_object()
                && !self.implements_as_interface_ref(tested_id, value_id, diagnostics)
        } else {
            true
        };
        if impossible {
            diagnostics.report_error(
                format!(
                    "'is {}' can never be true for a value of type {}",
                    self.ty_display(tested),
                    self.ty_display(value_ty)
                ),
                span,
            );
        }
    }

    /// Reports a non-`int` array index. Returns false when the index was rejected.
    pub(in crate::analyzer) fn check_array_index(
        &mut self,
        index_ty: &Type,
        span: Option<dream_text::text_span::TextSpan>,
        diagnostics: &mut dream_diagnostics::DiagnosticBag,
    ) -> bool {
        if index_ty.is_unknown() || index_ty.is_int() {
            return true;
        }
        diagnostics.report_error(
            format!(
                "Array index must be of type int, got {}",
                self.ty_display(index_ty)
            ),
            span,
        );
        false
    }

    /// Reports a `js` property key that is neither a string nor a number. Returns false when the
    /// key was rejected.
    pub(in crate::analyzer) fn check_js_index_key(
        &mut self,
        key_ty: &Type,
        span: Option<dream_text::text_span::TextSpan>,
        diagnostics: &mut dream_diagnostics::DiagnosticBag,
    ) -> bool {
        if key_ty.is_unknown() || key_ty.is_string() || key_ty.is_numeric() {
            return true;
        }
        diagnostics.report_error(
            format!(
                "a js property key must be a string or a number, got {}",
                self.ty_display(key_ty)
            ),
            span,
        );
        false
    }

    /// True when `def` is the core-language declaration named by a qualified identity
    /// (`system.core::Option`). A same-named user declaration does not match.
    pub(in crate::analyzer) fn is_core_def(&self, def: dream_types::DefId, identity: &str) -> bool {
        let kind = self.type_ctx.defs.get(def).kind;
        self.type_ctx.resolve(kind, identity) == Some(def)
    }
}
