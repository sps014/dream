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

    /// Checks explicit type visibility and rejects static classes in value positions.
    pub(in crate::analyzer) fn check_declared_type(
        &self,
        ty: &Type,
        parameters: &[String],
        diagnostics: &mut DiagnosticBag,
    ) {
        match ty {
            Type::Struct(token, args) => {
                if args.is_none() && parameters.contains(&token.text) {
                    return;
                }
                // Storage synthesized by lowering and the `fun(ref T)` grammar is not a source type name.
                if token.position.end > token.position.start && token.kind != TokenKind::RefToken {
                    let caller = diagnostics.file_path.as_deref().map(Rc::<str>::from);
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
                }
                if let Some(args) = args {
                    for a in args {
                        self.check_declared_type(a, parameters, diagnostics);
                    }
                }
            }
            Type::Array(inner) => self.check_declared_type(inner, parameters, diagnostics),
            Type::Tuple(elems) => {
                for e in elems {
                    self.check_declared_type(e, parameters, diagnostics);
                }
            }
            Type::Function(params, ret) => {
                for p in params {
                    self.check_declared_type(p, parameters, diagnostics);
                }
                self.check_declared_type(ret, parameters, diagnostics);
            }
            _ => {}
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

    /// True when `def` is the core-language declaration named by a qualified identity
    /// (`system.core::Option`). A same-named user declaration does not match.
    pub(in crate::analyzer) fn is_core_def(&self, def: dream_types::DefId, identity: &str) -> bool {
        let kind = self.type_ctx.defs.get(def).kind;
        self.type_ctx.resolve(kind, identity) == Some(def)
    }
}
