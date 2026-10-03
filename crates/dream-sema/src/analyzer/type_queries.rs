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

    /// True when `@json` derive (or a built-in JSON leaf / collection of those) can encode `ty`.
    /// Used by `Json.serialize` so analysis without the generator (LSP) still accepts
    /// `Map<string, string>` and still rejects `object`.
    pub(in crate::analyzer) fn json_type_encodable(&self, ty: dream_types::TypeId) -> bool {
        use dream_types::{PrimTy, TyKind};
        match self.type_ctx.interner.kind(ty) {
            TyKind::Error => true,
            TyKind::Prim(p) => matches!(
                p,
                PrimTy::Int
                    | PrimTy::UInt
                    | PrimTy::Long
                    | PrimTy::ULong
                    | PrimTy::Byte
                    | PrimTy::Float
                    | PrimTy::Double
                    | PrimTy::Bool
                    | PrimTy::String
            ),
            TyKind::Array(elem) => self.json_type_encodable(*elem),
            TyKind::Tuple(elems) => elems.iter().all(|e| self.json_type_encodable(*e)),
            TyKind::Struct(def, args) | TyKind::Union(def, args) => {
                let name = self.type_ctx.defs.name(*def);
                let builtin = self.program.structs.iter().any(|declaration| {
                    declaration.name.text == name
                        && self.graph.module_for_file(declaration.file_path.as_deref())
                            == def.module
                        && declaration
                            .file_path
                            .as_deref()
                            .is_some_and(dream_stdlib::is_std_source)
                }) || self.program.enums.iter().any(|declaration| {
                    declaration.name.text == name
                        && self.graph.module_for_file(declaration.file_path.as_deref())
                            == def.module
                        && declaration
                            .file_path
                            .as_deref()
                            .is_some_and(dream_stdlib::is_std_source)
                });
                if builtin && name == "JsonValue" {
                    return true;
                }
                if builtin && matches!(name, "List" | "Set" | "Option") {
                    return args.len() == 1 && self.json_type_encodable(args[0]);
                }
                if builtin && matches!(name, "Map" | "SortedMap") {
                    return args.len() == 2
                        && matches!(
                            self.type_ctx.interner.kind(args[0]),
                            TyKind::Prim(PrimTy::String)
                        )
                        && self.json_type_encodable(args[1]);
                }
                self.json_decl_has_attr(*def)
            }
            TyKind::Enum(def) => self.json_decl_has_attr(*def),
            _ => false,
        }
    }

    pub(in crate::analyzer) fn json_decl_has_attr(&self, def: dream_types::DefId) -> bool {
        let is_json = |attrs: &[dream_syntax::nodes::AttributeNode]| {
            attrs.iter().any(|attribute| attribute.name.text == "json")
        };
        let info = self.type_ctx.defs.get(def);
        self.program.structs.iter().any(|declaration| {
            declaration.name.text == info.name
                && self.graph.module_for_file(declaration.file_path.as_deref()) == def.module
                && is_json(&declaration.attributes)
        }) || self.program.enums.iter().any(|declaration| {
            declaration.name.text == info.name
                && self.graph.module_for_file(declaration.file_path.as_deref()) == def.module
                && is_json(&declaration.attributes)
        })
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

    /// Rejects using a `static class` as a value type (annotations, fields, generic args, arrays).
    pub(in crate::analyzer) fn check_type_not_static_class(
        &self,
        ty: &Type,
        diagnostics: &mut DiagnosticBag,
    ) {
        match ty {
            Type::Struct(token, args) => {
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
                        self.check_type_not_static_class(a, diagnostics);
                    }
                }
            }
            Type::Array(inner) => self.check_type_not_static_class(inner, diagnostics),
            Type::Tuple(elems) => {
                for e in elems {
                    self.check_type_not_static_class(e, diagnostics);
                }
            }
            Type::Function(params, ret) => {
                for p in params {
                    self.check_type_not_static_class(p, diagnostics);
                }
                self.check_type_not_static_class(ret, diagnostics);
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
}
