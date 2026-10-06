//! Struct/class registration: field layout, value-vs-reference classification, value-containment
//! soundness checks, and generic-struct instantiation.

use super::*;
use dream_syntax::nodes::struct_node::{StructDeclarationNode, StructFieldNode};

impl<'a> Analyzer<'a> {
    /// Pass 0: register every (non-generic) struct and its methods; stash generic templates.
    pub(in crate::analyzer) fn register_structs(
        &mut self,
        node: &'a ProgramView<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        // Every struct name must lower to its own type before any method registers: overloaded
        // methods are keyed by their parameter `TypeId`s, and a struct declared later in the
        // merged program (a stdlib `CPtr` parameter on a user overload) would otherwise key as
        // the poison type at registration but as the real struct when its body is emitted.
        for struct_decl in node.structs.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(struct_decl.file_path.as_deref()));
            self.type_ctx.register(
                DefKind::Struct,
                &struct_decl.name.text,
                generic_param_names(&struct_decl.generic_parameters),
            );
        }
        // Conformance checks may ask whether a later-declared class (`HttpHeadersIterator`)
        // implements an interface, so every declared clause is visible before any is validated.
        for struct_decl in node.structs.iter() {
            if struct_decl.generic_parameters.is_some() || struct_decl.implements.is_empty() {
                continue;
            }
            self.type_ctx
                .set_scope(self.graph.module_for_file(struct_decl.file_path.as_deref()));
            let Some(def) = self
                .type_ctx
                .resolve(DefKind::Struct, &struct_decl.name.text)
            else {
                continue;
            };
            let ty = self.type_ctx.interner.struct_ty(def, vec![]);
            let lowered: Vec<_> = struct_decl
                .implements
                .iter()
                .map(|iface| self.type_ctx.lower(iface))
                .collect();
            let declared = lowered
                .into_iter()
                .filter(|&iface| self.is_interface_name(iface))
                .collect();
            self.implements.insert(ty, declared);
        }
        for struct_decl in node.structs.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(struct_decl.file_path.as_deref()));
            diagnostics.file_path = file_path_string(&struct_decl.file_path);
            // Static classes are implicitly `sealed` on the AST so they cannot grow an instance
            // surface, but stdlib splits static helpers across `extend` files (e.g. `Math`).
            // Those extends are allowed when every member is `static` (checked in
            // `register_extensions`).
            let def = self.type_ctx.register(
                DefKind::Struct,
                &struct_decl.name.text,
                generic_param_names(&struct_decl.generic_parameters),
            );
            if struct_decl.is_sealed && !struct_decl.is_static {
                self.sealed_types.insert(def);
            }
            if struct_decl.is_static {
                self.type_ctx.defs.mark_static(def);
                self.type_ctx.interner.mark_static_def(def);
                self.validate_static_class(struct_decl, diagnostics);
            }
            if struct_decl.is_shared {
                self.type_ctx.interner.mark_shared_def(def);
            }
            if struct_decl.generic_parameters.is_some() {
                // A generic class may implement a (generic or non-generic) interface; the
                // `implements` clause is validated per monomorphization in `ensure_struct_instantiated`.
                // Async methods are supported: each monomorphization registers the method as a
                // distinct concrete function (see `register_struct_methods`), so its async state
                // machine is generated per instance like any other async method.
                continue;
            }
            let ty = self.type_ctx.interner.struct_ty(def, vec![]);
            let field_types: Vec<_> = struct_decl
                .fields
                .iter()
                .map(|field| self.type_ctx.lower(&field.field_type))
                .collect();
            if let Err(e) = self.struct_table.add_struct(ty, struct_decl, &field_types) {
                diagnostics.report_error(e, Some(struct_decl.name.position));
            }
            self.register_struct_methods(struct_decl, ty, &GenericBindings::new(), diagnostics);
            self.implements.shift_remove(&ty);
            self.validate_implements(
                ty,
                &struct_decl.implements,
                &struct_decl.methods,
                &GenericBindings::new(),
                struct_decl.name.position,
                diagnostics,
            );
        }

        // A value (`struct`) type is stored inline, so it cannot (transitively) contain itself by
        // value — that would require infinite storage. A reference (`class`) or array field breaks
        // the cycle. Generic value structs are checked per instantiation.
        for struct_decl in node.structs.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(struct_decl.file_path.as_deref()));
            if struct_decl.generic_parameters.is_some() {
                continue;
            }
            let name = &struct_decl.name.text;
            let ty = self
                .type_ctx
                .resolved_type(name)
                .unwrap_or_else(|| self.type_ctx.interner.error());
            let is_value = self.struct_info(ty).map(|s| s.is_value).unwrap_or(false);
            if is_value && self.value_struct_contains_self(ty) {
                diagnostics.report_error(
                    format!(
                        "value struct '{}' cannot contain itself by value; use a reference type ('class') or an array to break the cycle",
                        name
                    ),
                    Some(struct_decl.name.position),
                );
            }
        }

        // `@shared class` closed-graph rule: every field must be safe to access from another
        // thread without going through this class's own lock — either unmanaged/value-typed
        // (copied, no shared heap pointer), itself another `@shared` type (guarded by its own
        // lock), or a managed heap reference whose whole graph joins this object's shared
        // region (accessed under the same `lock` discipline). Run once every non-generic
        // class's `is_shared`/fields are registered above, so a field referencing another
        // `@shared` class declared later in the same file still resolves.
        for struct_decl in node.structs.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(struct_decl.file_path.as_deref()));
            if struct_decl.generic_parameters.is_some() {
                continue;
            }
            if !struct_decl.is_shared {
                continue;
            }
            let owner_name = struct_decl.name.text.clone();
            for field in &struct_decl.fields {
                self.check_shared_field(&owner_name, field, diagnostics);
            }
        }

        // `weak`/`unowned` field validation and the whole-program class reference-cycle check run
        // last, once every non-generic class's fields are in `self.struct_table` (needed to
        // classify a field's target as a value struct vs. a class).
        self.check_weak_unowned_and_cycles(node, diagnostics);

        // A `ref struct` field would smuggle a stack-only value into a heap-allocated (or
        // otherwise longer-lived) container — reject it regardless of whether the enclosing type
        // is a `class` or a `struct`. Run once every struct's own `is_ref_struct`/`is_value` marks
        // are registered (the loop above), so a field referencing another `ref struct` declared
        // later in the same file still resolves correctly.
        for struct_decl in node.structs.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(struct_decl.file_path.as_deref()));
            for field in &struct_decl.fields {
                self.reject_ref_struct_field(&struct_decl.name.text, field, diagnostics);
                self.check_type_not_static_class(&field.field_type, diagnostics);
            }
        }
    }

    fn validate_static_class(
        &self,
        struct_decl: &StructDeclarationNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let name = &struct_decl.name.text;
        if !struct_decl.fields.is_empty() {
            diagnostics.report_error(
                format!("static class '{}' cannot have instance fields", name),
                Some(struct_decl.fields[0].name.position),
            );
        }
        if !struct_decl.implements.is_empty() {
            diagnostics.report_error(
                format!("static class '{}' cannot implement interfaces", name),
                Some(struct_decl.name.position),
            );
        }
        for method in &struct_decl.methods {
            if dream_syntax::nodes::types::is_special_member_name(&method.name.text) {
                diagnostics.report_error(
                    format!(
                        "static class '{}' cannot declare '{}'",
                        name, method.name.text
                    ),
                    Some(method.name.position),
                );
            } else if !method.is_static {
                diagnostics.report_error(
                    format!(
                        "member '{}' of static class '{}' must be static",
                        method.name.text, name
                    ),
                    Some(method.name.position),
                );
            }
        }
    }

    /// Classifies one field of an `@shared class`. Shared-typed fields are ordinary. A
    /// non-shared field whose type is a managed heap reference (another class, an array like
    /// `int[]`, `List<T>`, a union such as `Option<Job>`, …) is also allowed: the object's whole
    /// reachable graph is treated as shared once inside an `@shared class`, so such fields are
    /// accessed under the same `lock` discipline as every other field — no per-field rules.
    /// Anything else (a value struct embedding a non-shared reference, unresolved generics) is
    /// rejected.
    fn check_shared_field(
        &mut self,
        owner_name: &str,
        field: &StructFieldNode,
        diagnostics: &mut DiagnosticBag,
    ) {
        if self.type_satisfies_kind(
            &field.field_type,
            dream_syntax::nodes::ConstraintKind::Shared,
        ) {
            return;
        }
        if self.shared_graph_field_ok(&field.field_type, 0, &field.name.position, diagnostics) {
            return;
        }
        diagnostics.report_error(
            format!(
                "field '{}' of 'shared class {}' has type '{}', which is not shared: a 'shared class' may only hold blittable values, string, structs of shared fields, other 'shared' types, or managed heap types ('Option<T>', arrays, classes) accessed under 'lock'",
                field.name.text,
                owner_name,
                self.ty_display(&field.field_type)
            ),
            Some(field.name.position),
        );
    }

    /// True when a value of type `ty` may live inside an `@shared class`'s shared region:
    /// either `shared`, a non-value class instance (whole object joins the graph), or an
    /// array/union/value-struct whose element/payload/field types are themselves joinable.
    /// The recursion is what blocks smuggling: `Option<Wrap>` where `Wrap` is a value struct
    /// embedding a non-shared class would copy an untracked pointer into the shared object.
    fn shared_graph_field_ok(
        &mut self,
        ty: &Type,
        depth: u32,
        position: &TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) -> bool {
        if depth > 8 || self.type_satisfies_kind(ty, dream_syntax::nodes::ConstraintKind::Shared) {
            return depth <= 8;
        }
        match ty {
            Type::Array(elem) => self.shared_graph_field_ok(elem, depth + 1, position, diagnostics),
            Type::Struct(token, args) => {
                let id = self.type_ctx.lower(ty);
                if let Some(info) = self.struct_info(id) {
                    // A reference class joins wholesale; a value struct's inline fields must
                    // each be joinable (their bytes — including embedded pointers — are copied).
                    if !info.is_value {
                        return true;
                    }
                    let field_types: Vec<Type> =
                        info.fields.values().map(|f| f.type_.clone()).collect();
                    return field_types
                        .iter()
                        .all(|f| self.shared_graph_field_ok(f, depth + 1, position, diagnostics));
                }
                if self.union_info(id).is_some() {
                    let base = token.text.clone();
                    let arg_types = args.clone().unwrap_or_default();
                    self.ensure_union_instantiated(&base, &arg_types, position, diagnostics);
                    let payload_types: Vec<Type> = self
                        .union_info(id)
                        .map(|info| {
                            info.variants
                                .iter()
                                .flat_map(|v| v.fields.iter().map(|f| f.type_.clone()))
                                .collect()
                        })
                        .unwrap_or_default();
                    return payload_types
                        .iter()
                        .all(|f| self.shared_graph_field_ok(f, depth + 1, position, diagnostics));
                }
                // Unresolvable here (not yet instantiated / unknown): don't block registration;
                // later concrete uses go through their own checks.
                true
            }
            _ => false,
        }
    }

    /// Reports an error if `field`'s type is a `ref struct` — such a type cannot be stored as a
    /// field of any enclosing type (`class` or `struct`), since that would let a stack-only value
    /// outlive the stack frame it was created in.
    pub(in crate::analyzer) fn reject_ref_struct_field(
        &mut self,
        owner_name: &str,
        field: &StructFieldNode,
        diagnostics: &mut DiagnosticBag,
    ) {
        let tid = self.type_ctx.lower(&field.field_type);
        if self.type_ctx.interner.is_ref_struct_type(tid) {
            diagnostics.report_error(
                format!(
                    "field '{}' of '{}' cannot have type '{}': a 'ref struct' cannot be stored as a field (it would let a stack-only value escape its stack frame)",
                    field.name.text,
                    owner_name,
                    self.ty_display(&field.field_type)
                ),
                Some(field.name.position),
            );
        }
    }

    /// Rejects a `ref struct`-typed parameter on any `async` function, method, or `extend`-block
    /// method in the program: an `async` call may suspend at an `await`, which spills the coroutine's
    /// live locals (including its parameters) into a heap-allocated state object so they survive
    /// across the suspend point — exactly the kind of stack-frame escape a `ref struct` forbids.
    /// Generic templates are checked once per instantiation's concrete parameter types would be
    /// ideal, but templates don't carry a `ref struct` argument until monomorphized, so this walks
    /// only concrete (non-generic) declarations, matching this analysis's stated conservative scope.
    pub(in crate::analyzer) fn check_ref_struct_async_boundary(
        &mut self,
        node: &'a ProgramView<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let check_fn = |this: &mut Self,
                        f: &dream_syntax::nodes::function::FunctionNode<'a>,
                        diags: &mut DiagnosticBag| {
            if !f.is_async {
                return;
            }
            for p in &f.parameters {
                let tid = this.type_ctx.lower(&p.type_);
                if this.type_ctx.interner.is_ref_struct_type(tid) {
                    diags.report_error(
                        format!(
                            "async function '{}' cannot take 'ref struct' parameter '{}' of type '{}': it may need to survive an 'await' suspend point, which would spill it into the heap-allocated coroutine state",
                            f.name.text,
                            p.name.text,
                            this.ty_display(&p.type_)
                        ),
                        Some(f.name.position),
                    );
                }
            }
        };
        for f in node.functions.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(f.file_path.as_deref()));
            check_fn(self, f, diagnostics);
        }
        for s in node.structs.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(s.file_path.as_deref()));
            for m in &s.methods {
                check_fn(self, m, diagnostics);
            }
        }
        for e in node.extends.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(e.file_path.as_deref()));
            for m in &e.methods {
                check_fn(self, m, diagnostics);
            }
        }
        for en in node.enums.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(en.file_path.as_deref()));
            for m in &en.methods {
                check_fn(self, m, diagnostics);
            }
        }
    }

    /// Rejects any `ref struct` type appearing in `args` as a generic type argument: instantiating
    /// a generic class/struct/union/function with a `ref struct` argument would store it in a field,
    /// array element, or heap payload somewhere in that generic's body, letting a stack-only value
    /// escape its frame. Called at every generic instantiation site (classes, unions, and — where
    /// wired — generic function calls).
    pub(in crate::analyzer) fn reject_ref_struct_type_args(
        &mut self,
        args: &[Type],
        position: &TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        for arg in args {
            let tid = self.type_ctx.lower(arg);
            if self.type_ctx.interner.is_ref_struct_type(tid) {
                diagnostics.report_error(
                    format!(
                        "'{}' is a 'ref struct' and cannot be used as a generic type argument (it would be stored in a heap-allocated container, letting it escape its stack frame)",
                        self.ty_display(arg)
                    ),
                    Some(*position),
                );
            }
        }
    }

    /// True when value struct `start` transitively embeds itself by value. Only value-typed,
    /// non-array fields form inline edges; reference fields (`class`, `string`, arrays) do not.
    fn value_struct_contains_self(&self, start: dream_types::TypeId) -> bool {
        let mut visited = indexmap::IndexSet::new();
        let mut work = self.value_struct_field_targets(start);
        while let Some(cur) = work.pop() {
            if cur == start {
                return true;
            }
            if !visited.insert(cur) {
                continue;
            }
            work.extend(self.value_struct_field_targets(cur));
        }
        false
    }

    /// Only inline edges contribute to infinite-size cycles; references and arrays break them.
    fn value_struct_field_targets(&self, ty: dream_types::TypeId) -> Vec<dream_types::TypeId> {
        if !self.type_ctx.interner.is_value_type(ty) {
            return Vec::new();
        }
        let fields = if let Some(info) = self.union_info(ty) {
            info.variants
                .iter()
                .flat_map(|v| v.fields.iter().map(|f| f.ty))
                .collect::<Vec<_>>()
        } else if let Some(info) = self.struct_info(ty) {
            info.fields.values().map(|f| f.ty).collect()
        } else if let dream_types::TyKind::Tuple(fields) = self.type_ctx.interner.kind(ty) {
            fields.clone()
        } else {
            Vec::new()
        };
        fields
            .into_iter()
            .filter(|t| self.type_ctx.interner.is_value_type(*t))
            .collect()
    }

    pub(in crate::analyzer) fn validate_value_containment(&self, diagnostics: &mut DiagnosticBag) {
        // An enum can instantiate a generic struct before its own payload table is complete.
        // Recheck once every concrete field graph is available, before computing layouts.
        for (ty, kind) in self.type_ctx.interner.iter_kinds() {
            if matches!(kind, dream_types::TyKind::Struct(..))
                && self.type_ctx.interner.is_value_type(ty)
                && self.value_struct_contains_self(ty)
            {
                diagnostics.report_error(format!("value struct '{}' cannot contain itself by value; use a reference type ('class') or an array to break the cycle", self.type_id_display(ty)), None);
            }
        }
    }

    pub(in crate::analyzer) fn ensure_struct_instantiated(
        &mut self,
        base_name: &str,
        args: &[Type],
        position: &TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        let Some(def) = self.type_ctx.resolve(DefKind::Struct, base_name) else {
            return;
        };
        let concrete_ids: Vec<_> = args.iter().map(|arg| self.type_ctx.lower(arg)).collect();
        let instance = self.type_ctx.instantiate(def, concrete_ids.clone());
        if self.struct_info(instance).is_some() {
            return;
        }
        let template = match self.generic_struct(def) {
            Some(template) => *template,
            None => return,
        };
        self.generic_struct_instances.insert(instance);
        let mangled_name =
            dream_types::display_name(&self.type_ctx.interner, &self.type_ctx.defs, instance);
        let args: Vec<_> = concrete_ids
            .iter()
            .map(|&arg| self.type_ctx.syntax_type(arg))
            .collect();
        let scope = self.type_ctx.scope();
        self.type_ctx.set_scope(def.module);
        let params = template.generic_parameters.as_deref().unwrap_or(&[]);
        Self::check_generic_arity(
            "class",
            base_name,
            params.len(),
            args.len(),
            position,
            diagnostics,
        );
        self.reject_ref_struct_type_args(&args, position, diagnostics);
        let bindings = generic_bindings(params, &args);
        let type_bindings: IndexMap<_, _> = params
            .iter()
            .zip(&concrete_ids)
            .map(|(parameter, &ty)| (parameter.text.clone(), ty))
            .collect();

        // A constrained class/struct parameter (`class Sorted<T : Comparable<T>>`) must be satisfied
        // by the concrete argument at this instantiation.
        self.verify_generic_constraints(
            &template.generic_constraints,
            &bindings,
            position,
            diagnostics,
        );
        if base_name == "Vector" {
            if let Some(elem) = args.first() {
                if !matches!(
                    elem,
                    Type::Byte(_)
                        | Type::Integer(_)
                        | Type::Long(_)
                        | Type::Float(_)
                        | Type::Double(_)
                        | Type::Unknown
                        | Type::Generic(_)
                ) {
                    diagnostics.report_error(
                        "'Vector<T>' requires T to be byte, int, long, float, or double"
                            .to_string(),
                        Some(*position),
                    );
                }
            }
        }

        let new_fields: Vec<StructFieldNode> = template
            .fields
            .iter()
            .map(|field| StructFieldNode {
                attributes: field.attributes.clone(),
                name: field.name.clone(),
                visibility: field.visibility,
                is_weak: field.is_weak,
                is_unowned: field.is_unowned,
                type_token: field.type_token.clone(),
                field_type: {
                    let ty = self.type_ctx.lower_with(&field.field_type, &type_bindings);
                    self.type_ctx.syntax_type(ty)
                },
            })
            .collect();

        let mut new_name_token = template.name.clone();
        new_name_token.text = mangled_name.clone();
        let mut new_decl = StructDeclarationNode::new(
            template.attributes.clone(),
            new_name_token,
            None,
            new_fields,
            template.methods.clone(),
            template.visibility,
        );
        new_decl.is_value = template.is_value;
        new_decl.is_ref_struct = template.is_ref_struct;
        new_decl.is_shared = template.is_shared;
        new_decl.file_path = template.file_path.clone();

        let new_decl_ref: &'a StructDeclarationNode<'a> = self.arena.alloc(new_decl);

        self.type_ctx
            .set_scope(self.graph.module_for_file(template.file_path.as_deref()));
        let field_types: Vec<_> = template
            .fields
            .iter()
            .map(|field| self.type_ctx.lower_with(&field.field_type, &type_bindings))
            .collect();
        if let Err(e) = self
            .struct_table
            .add_struct(instance, new_decl_ref, &field_types)
        {
            diagnostics.report_error(e, Some(*position));
        }

        // The closed-graph rule runs per monomorphization for generic `@shared class`es: whether a
        // field like `payload: T` is shared, transfer-only (`Option<T>`), or rejected is only
        // decidable once `T` is concrete.
        if template.is_shared {
            let owner = mangled_name.clone();
            for field in &new_decl_ref.fields {
                self.check_shared_field(&owner, field, diagnostics);
            }
        }

        // Value-struct soundness is checked per instantiation (the template's fields are generic, so
        // whether this monomorphization embeds itself by value is only decidable once `T` is
        // concrete).
        if new_decl_ref.is_value && self.value_struct_contains_self(instance) {
            diagnostics.report_error(
                    format!(
                        "value struct '{}' cannot contain itself by value; use a reference type ('class') or an array to break the cycle",
                        mangled_name
                    ),
                    Some(*position),
                );
        }

        self.register_struct_methods(new_decl_ref, instance, &bindings, diagnostics);
        self.register_generic_extension_methods(
            GenericExtendTarget::Nominal(def),
            instance,
            &args,
            diagnostics,
        );

        // Validate this monomorphization's `implements` clause: substitute the class type parameters
        // through each listed interface (`Container<T>` -> `Container<int>`) and match the (also
        // substituted) method signatures. Records `implements[Box_int] = [Container_int]`.
        if !template.implements.is_empty() {
            let sub_impls: Vec<Type> = template
                .implements
                .iter()
                .map(|t| substitute_generic_type(t, &bindings))
                .collect();
            self.validate_implements(
                instance,
                &sub_impls,
                &template.methods,
                &bindings,
                *position,
                diagnostics,
            );
        }
        self.type_ctx.set_scope(scope);
    }
}
