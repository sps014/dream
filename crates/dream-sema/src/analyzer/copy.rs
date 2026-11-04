//! `@copy` types: the compiler-derived deep `copy()` of a class, `Copy.of<T>` for generic stdlib
//! code, and the checks that every unique value a copy duplicates is itself copyable.

use super::*;
use crate::module_graph::ProgramView;
use dream_diagnostics::{Diagnostic, DiagnosticBag};
use dream_hir::{Binding, Callee, HExpr, HExprKind, HPlace, HStmt};
use dream_syntax::nodes::struct_node::StructDeclarationNode;
use dream_syntax::nodes::{FunctionNode, Visibility};
use dream_text::text_span::TextSpan;
use dream_types::{TyKind, TypeId};

const COPY_METHOD: &str = "copy";
const COPY_LOCAL: &str = "__copy";

impl<'a> Analyzer<'a> {
    /// The `copy()` the compiler writes for a `@copy` class that declares none of its own.
    pub(in crate::analyzer) fn derived_copy_method(
        &self,
        decl: &'a StructDeclarationNode<'a>,
        struct_type: TypeId,
    ) -> Option<&'a [FunctionNode<'a>]> {
        if !dream_abi::attributes::is_copy(&decl.attributes)
            || decl.is_static
            || decl.methods.iter().any(|m| m.name.text == COPY_METHOD)
        {
            return None;
        }
        let mut method = FunctionNode::new(
            Vec::new(),
            synthetic_token(TokenKind::IdentifierToken, COPY_METHOD),
            None,
            Some(self.type_ctx.syntax_type(struct_type)),
            Vec::new(),
            &[],
            Visibility::Public,
        );
        method.name.position = decl.name.position;
        method.file_path = decl.file_path.clone();
        Some(std::slice::from_ref(self.arena.alloc(method)))
    }

    pub(in crate::analyzer) fn note_derived_copy(&mut self, struct_type: TypeId) {
        if let Some(identity) = self
            .function_table
            .method_candidates(struct_type, COPY_METHOD)
            .first()
        {
            self.derived_copies.insert(identity.0);
        }
    }

    pub(in crate::analyzer) fn is_derived_copy(&mut self, function: &FunctionNode<'a>) -> bool {
        !self.derived_copies.is_empty()
            && self
                .function_declaration(function)
                .is_some_and(|identity| self.derived_copies.contains(&identity.0))
    }

    /// Body of a derived `copy()`: a fresh instance whose fields are copies of `this`'s. Weak and
    /// unowned fields keep pointing at the same object, since they never owned it.
    pub(in crate::analyzer) fn hir_emit_derived_copy(&mut self) {
        let Some((this, this_ty)) = self.hir_local_slot(dream_abi::names::THIS_NAME) else {
            self.hir_fail();
            return;
        };
        let TyKind::Struct(def, _) = self.type_ctx.interner.kind(this_ty).clone() else {
            self.hir_fail();
            return;
        };
        let fields: Vec<(TypeId, bool)> = self
            .struct_info(this_ty)
            .map(|info| {
                info.fields
                    .values()
                    .map(|f| (f.ty, f.is_weak || f.is_unowned))
                    .collect()
            })
            .unwrap_or_default();
        let Some(copy) = self.hir_alloc_local(COPY_LOCAL, &self.type_ctx.syntax_type(this_ty))
        else {
            return;
        };
        self.hir_push_stmt(HStmt::Let {
            local: copy,
            ty: this_ty,
            value: HExpr::new(
                this_ty,
                HExprKind::New {
                    def,
                    instance: vec![],
                    ctor: None,
                    args: vec![],
                    take_params: vec![],
                },
            ),
        });
        for (index, (field_ty, unowned)) in fields.into_iter().enumerate() {
            let read = HExpr::new(
                field_ty,
                HExprKind::Field {
                    obj: Box::new(HExpr::new(this_ty, HExprKind::Var(Binding::Local(this)))),
                    field: index,
                },
            );
            let value = if unowned {
                read
            } else {
                self.copy_expr(read, field_ty)
            };
            self.hir_push_stmt(HStmt::Assign {
                place: HPlace::Field {
                    obj: Box::new(HExpr::new(this_ty, HExprKind::Var(Binding::Local(copy)))),
                    field: index,
                },
                value,
            });
        }
        self.hir_push_stmt(HStmt::Return(Some(HExpr::new(
            this_ty,
            HExprKind::Var(Binding::Local(copy)),
        ))));
    }

    /// `value.copy()` when `ty` owns unique data, otherwise `value` itself. Callers validate
    /// copyability first, so a type without a `copy` method only reaches here in an instance no
    /// validated call can run.
    fn copy_expr(&self, value: HExpr, ty: TypeId) -> HExpr {
        if !self.needs_copy(ty) {
            return value;
        }
        let Some(identity) = self
            .function_table
            .method_candidates(ty, COPY_METHOD)
            .into_iter()
            .next()
        else {
            return value;
        };
        let take_params = self
            .function_table
            .functions
            .get(&identity)
            .map(|info| info.is_take.clone())
            .unwrap_or_default();
        HExpr::new(
            ty,
            HExprKind::MethodCall {
                receiver: Box::new(value),
                callee: Callee {
                    def: identity.0,
                    instance: identity.1,
                    ret: ty,
                    take_params,
                },
                args: vec![],
            },
        )
    }

    /// `Copy.of<T>(value)`: copies `value` as its concrete instantiation requires.
    pub(in crate::analyzer) fn analyze_copy_of(
        &mut self,
        position: TextSpan,
        generic_args: Option<&Type>,
        types: Vec<TypeId>,
        hirs: Vec<Option<HExpr>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Type {
        if !self.current_function_is_trusted_prelude() || types.len() != 1 {
            diagnostics.report_error(
                "'Copy.of' is internal and expects one argument".to_string(),
                Some(position),
            );
            self.hir_none();
            return Type::Unknown;
        }
        let ty = match generic_args {
            Some(arg) => {
                let arg = Self::monomorphize_type(arg, &self.current_generic_bindings);
                self.type_ctx.lower(&arg)
            }
            None => types[0],
        };
        let value = hirs
            .into_iter()
            .next()
            .flatten()
            .map(|v| self.copy_expr(v, ty));
        self.hir_set_last(value);
        self.type_ctx.syntax_type(ty)
    }

    /// Rejects `.copy()` on a `@copy` type whose copy would reach a value that cannot be copied
    /// (for example `List<Node>` when `Node` is not `@copy`).
    pub(in crate::analyzer) fn check_copy_call(
        &self,
        owner: TypeId,
        member: &str,
        position: TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        if member != COPY_METHOD || !self.is_copy_type(owner) {
            return;
        }
        if let Some(blocker) = self.copy_blocker(owner) {
            let diagnostic = Diagnostic::new(
                format!(
                    "'{}' cannot be copied: {}",
                    self.type_id_display(owner),
                    self.blocker_reason(blocker)
                ),
                Some(position),
                diagnostics.file_path.clone(),
            );
            diagnostics.report(self.with_copy_help(diagnostic, blocker));
        }
    }

    pub(in crate::analyzer) fn missing_copy_note(
        &self,
        owner: TypeId,
        member: &str,
    ) -> Option<String> {
        let (TyKind::Struct(def, _) | TyKind::Union(def, _)) = self.type_ctx.interner.kind(owner)
        else {
            return None;
        };
        (member == COPY_METHOD && self.needs_copy(owner)).then(|| {
            format!(
                "add @copy to '{}' so its values can be copied",
                self.type_ctx.defs.name(*def)
            )
        })
    }

    /// What a use-after-move diagnostic suggests for keeping `name` (of type `ty`) usable.
    pub(in crate::analyzer) fn ownership_help(&self, name: &str, ty: TypeId) -> String {
        let display = self.type_id_display(ty);
        let has_copy = !self
            .function_table
            .method_candidates(ty, COPY_METHOD)
            .is_empty();
        if self.is_copy_type(ty) && has_copy && self.copy_blocker(ty).is_none() {
            return format!(
                "pass '{name}.copy()' where it moves to keep using '{name}'; a '{display}' value has one owner"
            );
        }
        let user_class = match self.type_ctx.interner.kind(ty) {
            TyKind::Struct(def, _) => self
                .type_visibility
                .get(def)
                .is_some_and(|(file, _)| {
                    !file
                        .as_deref()
                        .is_some_and(dream_stdlib::is_std_source)
                })
                .then(|| self.type_ctx.defs.name(*def).to_string()),
            _ => None,
        };
        match user_class {
            Some(class) => format!(
                "a '{display}' value has one owner; add @copy to '{class}' to pass copies, or declare it 'shared class' if several places must use the same object"
            ),
            None => format!("a '{display}' value has one owner"),
        }
    }

    /// Declaration checks: every field of a non-generic `@copy` class must be copyable, and a
    /// `@copy` union must write its own `copy()`.
    pub(in crate::analyzer) fn check_copy_declarations(
        &mut self,
        node: &'a ProgramView<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        for decl in node.structs.iter() {
            if !dream_abi::attributes::is_copy(&decl.attributes)
                || decl.generic_parameters.is_some()
            {
                continue;
            }
            diagnostics.file_path = file_path_string(&decl.file_path);
            self.check_custom_copy_signature(&decl.methods, diagnostics);
            self.type_ctx
                .set_scope(self.graph.module_for_file(decl.file_path.as_deref()));
            let Some(ty) = self.type_ctx.resolved_type(&decl.name.text) else {
                continue;
            };
            if !self.function_table.method_candidates(ty, COPY_METHOD).first().is_some_and(
                |identity| self.derived_copies.contains(&identity.0),
            ) {
                continue;
            }
            let fields: Vec<(String, TypeId)> = self
                .struct_info(ty)
                .map(|info| {
                    info.fields
                        .iter()
                        .filter(|(_, f)| !f.is_weak && !f.is_unowned)
                        .map(|(name, f)| (name.clone(), f.ty))
                        .collect()
                })
                .unwrap_or_default();
            for (name, field_ty) in fields {
                let Some(blocker) = self.copy_blocker(field_ty) else {
                    continue;
                };
                let position = decl
                    .fields
                    .iter()
                    .find(|f| f.name.text == name)
                    .map_or(decl.name.position, |f| f.name.position);
                let diagnostic = Diagnostic::new(
                    format!(
                        "field '{name}' of @copy class '{}' cannot be copied: {}",
                        decl.name.text,
                        self.blocker_reason(blocker)
                    ),
                    Some(position),
                    diagnostics.file_path.clone(),
                );
                diagnostics.report(self.with_copy_help(diagnostic, blocker));
            }
        }
        for decl in node.enums.iter() {
            if !dream_abi::attributes::is_copy(&decl.attributes) {
                continue;
            }
            diagnostics.file_path = file_path_string(&decl.file_path);
            self.type_ctx
                .set_scope(self.graph.module_for_file(decl.file_path.as_deref()));
            let owns_nothing = decl.generic_parameters.is_none()
                && self
                    .type_ctx
                    .resolved_type(&decl.name.text)
                    .is_some_and(|ty| !self.needs_copy(ty));
            if owns_nothing {
                diagnostics.report_error(
                    format!(
                        "'{}' holds no value with a single owner, so it is shared without copying; remove @copy",
                        decl.name.text
                    ),
                    Some(decl.name.position),
                );
            } else if decl.methods.iter().any(|m| m.name.text == COPY_METHOD) {
                self.check_custom_copy_signature(&decl.methods, diagnostics);
            } else {
                diagnostics.report_error(
                    format!(
                        "@copy union '{}' must declare 'public fun copy(): {}'",
                        decl.name.text, decl.name.text
                    ),
                    Some(decl.name.position),
                );
            }
        }
    }

    fn check_custom_copy_signature(
        &self,
        methods: &[FunctionNode<'a>],
        diagnostics: &mut DiagnosticBag,
    ) {
        for method in methods.iter().filter(|m| m.name.text == COPY_METHOD) {
            if method.is_static
                || method.is_async
                || !method.parameters.is_empty()
                || !method.visibility.is_public()
            {
                diagnostics.report_error(
                    "'copy' of a @copy type must be a public instance method without parameters"
                        .to_string(),
                    Some(method.name.position),
                );
            }
        }
    }

    fn is_copy_type(&self, ty: TypeId) -> bool {
        matches!(self.type_ctx.interner.kind(ty),
            TyKind::Struct(def, _) | TyKind::Union(def, _) if self.copy_types.contains(def))
    }

    /// True when `ty` owns a unique value, directly or through value structs, unions, arrays, or
    /// tuples. Shared classes, interfaces, and closures are reference handles and are not copied.
    fn needs_copy(&self, ty: TypeId) -> bool {
        self.owns_unique(ty, &mut HashSet::new())
    }

    fn owns_unique(&self, ty: TypeId, seen: &mut HashSet<TypeId>) -> bool {
        if !seen.insert(ty) {
            return false;
        }
        let interner = &self.type_ctx.interner;
        if interner.is_unique_type(ty) {
            return true;
        }
        let parts: Vec<TypeId> = match interner.kind(ty) {
            TyKind::Struct(..) => self
                .struct_info(ty)
                .filter(|info| info.is_value)
                .map(|info| info.fields.values().map(|f| f.ty).collect())
                .unwrap_or_default(),
            TyKind::Union(..) => self
                .union_info(ty)
                .map(|info| {
                    info.variants
                        .iter()
                        .flat_map(|v| v.fields.iter().map(|f| f.ty))
                        .collect()
                })
                .unwrap_or_default(),
            TyKind::Array(element) => vec![*element],
            TyKind::Tuple(elements) => elements.clone(),
            _ => Vec::new(),
        };
        parts.into_iter().any(|part| self.owns_unique(part, seen))
    }

    /// The first type reachable from `ty` that owns unique data but has no `copy()`.
    fn copy_blocker(&self, ty: TypeId) -> Option<TypeId> {
        self.copy_blocker_in(ty, &mut HashSet::new())
    }

    fn copy_blocker_in(&self, ty: TypeId, seen: &mut HashSet<TypeId>) -> Option<TypeId> {
        if !seen.insert(ty) || !self.needs_copy(ty) {
            return None;
        }
        let args = match self.type_ctx.interner.kind(ty) {
            TyKind::Struct(def, args) | TyKind::Union(def, args)
                if self.copy_types.contains(def) =>
            {
                args.clone()
            }
            _ => return Some(ty),
        };
        let derived = self
            .function_table
            .method_candidates(ty, COPY_METHOD)
            .first()
            .is_some_and(|identity| self.derived_copies.contains(&identity.0));
        let parts: Vec<TypeId> = if derived {
            self.struct_info(ty)
                .map(|info| {
                    info.fields
                        .values()
                        .filter(|f| !f.is_weak && !f.is_unowned)
                        .map(|f| f.ty)
                        .collect()
                })
                .unwrap_or_default()
        } else {
            args
        };
        parts
            .into_iter()
            .find_map(|part| self.copy_blocker_in(part, seen))
    }

    fn blocker_reason(&self, blocker: TypeId) -> String {
        let name = self.type_id_display(blocker);
        match self.type_ctx.interner.kind(blocker) {
            TyKind::Array(_) => format!("the array '{name}' holds values with one owner"),
            TyKind::Tuple(_) => format!("the tuple '{name}' holds values with one owner"),
            _ => format!("'{name}' is not marked @copy"),
        }
    }

    fn with_copy_help(&self, diagnostic: Diagnostic, blocker: TypeId) -> Diagnostic {
        match self.type_ctx.interner.kind(blocker) {
            TyKind::Struct(def, _) | TyKind::Union(def, _) => {
                let name = self.type_ctx.defs.name(*def).to_string();
                diagnostic.with_help(format!("add @copy to '{name}'"))
            }
            _ => diagnostic.with_help("store the values in a List instead".to_string()),
        }
    }
}
