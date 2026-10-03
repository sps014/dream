//! Method and extension registration: registering a struct's own methods and `extend`-block methods
//! (`register_methods_for`), the set of extendable targets, stashing generic extension templates,
//! and validating object-protocol overrides and property accessors. These are `impl Analyzer`
//! methods, kept in the `declarations` module alongside the other top-level registration passes.

use super::*;
use crate::function_table::FunctionTableInfo;
use crate::module_graph::ProgramView;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::struct_node::StructDeclarationNode;
use dream_syntax::nodes::types::PRIMITIVE_TYPE_NAMES;
use dream_syntax::nodes::{FunctionNode, Type};
use dream_types::method_fn;

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn register_struct_methods(
        &mut self,
        struct_decl: &'a StructDeclarationNode<'a>,
        struct_type: dream_types::TypeId,
        bindings: &GenericBindings,
        diagnostics: &mut DiagnosticBag,
    ) {
        let scope = self.type_ctx.scope();
        self.type_ctx
            .set_scope(self.graph.module_for_file(struct_decl.file_path.as_deref()));
        self.register_methods_for(struct_type, &struct_decl.methods, bindings, diagnostics);
        self.type_ctx.set_scope(scope);
    }

    /// Registers a list of methods against `target_type_str` (a struct, a monomorphized generic
    /// struct, or a primitive/`object` extended via an `extend` block). Each method is renamed to
    /// `{target}_{method}`, given an implicit `this` parameter of the target type, queued for
    /// codegen, and recorded in the function table. Shared by struct declarations and `extend`
    /// blocks so they lower identically.
    pub(in crate::analyzer) fn register_methods_for(
        &mut self,
        target_type: dream_types::TypeId,
        methods: &'a [FunctionNode<'a>],
        bindings: &GenericBindings,
        diagnostics: &mut DiagnosticBag,
    ) {
        let target_type_str = self.type_ctx.syntax_type(target_type).get_type();
        // Collect the mangled name + full parameter list (with the implicit `this`) of each method so
        // overloaded methods can be registered under their signature-mangled *emitted* names in a
        // second pass, once the whole overload set for this target is known.
        //
        // When a `where`-constrained method is satisfied for this instantiation, it wins over an
        // unconstrained twin with the same name + parameter types (e.g. `Span.copy_from` keeps the
        // element loop for reference `T`, and the unmanaged specialization that bulk-blits).
        let specialized_keys: indexmap::IndexSet<(String, Vec<String>)> = methods
            .iter()
            .filter(|m| {
                !m.where_constraints.is_empty()
                    && self.extension_constraints_satisfied(&m.where_constraints, bindings)
            })
            .map(|m| {
                let name = accessor_member_name(m);
                let params: Vec<String> = m.parameters.iter().map(|p| p.type_.get_type()).collect();
                (name, params)
            })
            .collect();
        for method in methods {
            // Conditional methods (`fun sort(): void where T : Comparable<T>`) only attach when
            // every where-bound is satisfied by this instantiation — same rule as constrained
            // `extend` blocks.
            if !method.where_constraints.is_empty()
                && !self.extension_constraints_satisfied(&method.where_constraints, bindings)
            {
                continue;
            }
            if let Some(ret) = &method.return_type {
                self.check_type_not_static_class(ret, diagnostics);
            }
            for p in &method.parameters {
                self.check_type_not_static_class(&p.type_, diagnostics);
            }
            let member_name = accessor_member_name(method);
            if method.where_constraints.is_empty() {
                let key = (
                    member_name.clone(),
                    method
                        .parameters
                        .iter()
                        .map(|p| p.type_.get_type())
                        .collect::<Vec<_>>(),
                );
                if specialized_keys.contains(&key) {
                    continue;
                }
            }
            // Validate object-protocol overrides once (on the non-monomorphized declaration).
            if bindings.is_empty() {
                self.validate_protocol_override(method, diagnostics);
                self.validate_accessor(method, diagnostics);
            }
            // Property accessors (`get`/`set`) are registered under a `$`-tagged internal name that a
            // user identifier can never spell, so `obj.prop`/`obj.prop = v` resolve to them without a
            // regular method (or the indexer `get`/`set` hooks) ever colliding.
            let mangled_name = method_fn(&target_type_str, &member_name);
            let mut new_method = method.clone();
            new_method.name = synthetic_token(TokenKind::IdentifierToken, &mangled_name);
            new_method.name.position = method.name.position;

            if !bindings.is_empty() {
                Self::substitute_generic_signature(&mut new_method, bindings);
            }
            let mut parameters: Vec<_> = new_method.parameters.iter().map(|p| self.type_ctx.lower(&p.type_)).collect();
            if !new_method.is_static { parameters.insert(0, target_type); }
            let def = self.type_ctx.register_method(target_type, &member_name, &parameters);

            // Register after substitution so generic `@operator` params are `Vector<int>`, not `Vector<T>`.
            let operator_method = new_method.clone();

            // Static methods have no implicit receiver; instance methods get `this` at index 0.
            if !new_method.is_static {
                new_method
                    .parameters
                    .insert(0, dream_syntax::nodes::ParameterNode::new(synthetic_token(TokenKind::IdentifierToken, "this"), self.type_ctx.syntax_type(target_type)));
            }

            // Stash the *renamed* clone (`{Type}_{method}`), not the raw declaration: its own
            // later deferred-body analysis uses this same node as `parent_function`, and
            // `in_methods_of`'s static-method check matches on the `{base}_` name prefix to grant
            // the method access to its own class's private members — the unrenamed original's bare
            // method name (e.g. `idx`, not `Helper_idx`) would never match that prefix.
            //
            // Method-level generics (`map<U>`, `spawn<TOut>`) mirror free generic functions:
            // stash the template only. Analyzing the unbound body would treat type params as
            // concrete names (e.g. `Option_U`) and emit the wrong calling convention for the
            // caller's monomorphized return type. Concrete instances are analyzed via
            // `instantiated_generics` after `register_generic_function_instance`.
            if method.generic_parameters.is_some() {
                let renamed_template: &'a FunctionNode<'a> = self.arena.alloc(new_method);
                self.type_ctx.defs.set_generic_params(def, generic_param_names(&method.generic_parameters));
                self.generic_functions.insert(def, renamed_template);
                self.function_table.generic_methods.insert((target_type, member_name), def);
                self.function_table.record_declaration(renamed_template, (def, Vec::new()));
                self.record_ide_definition(def, &method.name, method.file_path.as_deref());
                continue;
            }

            let method_ref = self.arena.alloc(new_method);
            self.struct_methods.push((method_ref, bindings.clone()));

            let mut info = FunctionTableInfo::from_identity(method_ref, (def, Vec::new()), &mut self.type_ctx);
            self.validate_and_register_operator(target_type, &operator_method, &info.identity, diagnostics);
            self.validate_and_register_protocol_hook(target_type, method, &info.identity, diagnostics);
            if bindings.is_empty() && method.is_extern && dream_abi::attributes::has_c_attr(&method.attributes) {
                self.validate_c_extern_signature(method, def, diagnostics);
            }
            self.record_ide_definition(info.identity.0, &method.name, method.file_path.as_deref());
            if let Some(key) = dream_abi::intrinsics::intrinsic_key(&method.attributes) { self.intrinsic_defs.push((info.identity.0, key)); }
            info.declaring_module = self.module_of(method_ref.file_path.as_ref());
            let identity = info.identity.clone();
            self.function_table.record_declaration(method_ref, identity);
            match self.function_table.add_method(target_type, &member_name, info) {
                Ok(_) => (),
                Err(error) => diagnostics.report_error(error.to_string(), Some(method.name.position)),
            }
        }

    }

    /// Returns true if `name` is a type that an `extend` block may attach methods to: a
    /// primitive (the shared [`PRIMITIVE_TYPE_NAMES`] list), the dynamic `object`/`js` reference
    /// types, an array of an extendable element (`int[]`), a registered struct, a generic struct
    /// template, an enum, or an interface (including generic interface templates —
    /// `extend Collection<T> { ... }`).
    pub(in crate::analyzer) fn is_extendable_target(&self, name: &str) -> bool {
        if let Some(elem) = name.strip_suffix("[]") {
            return self.is_extendable_target(elem);
        }
        PRIMITIVE_TYPE_NAMES.contains(&name)
            || matches!(name, "object" | "js")
            || self.type_ctx.resolved_type(name).is_some_and(|ty| self.struct_info(ty).is_some())
            || self.type_ctx.resolve(DefKind::Struct, name).is_some_and(|def| self.generic_struct(def).is_some())
            || self.type_ctx.resolve(DefKind::Enum, name).is_some_and(|def| self.enum_members(def).is_some())
            || self.type_ctx.resolve(DefKind::Interface, name).is_some_and(|def| self.interface_decl(def).is_some())
    }

    fn extend_target_type(&mut self, name: &str) -> Option<dream_types::TypeId> {
        match name.strip_suffix("[]") {
            Some(elem) => {
                let elem = self.extend_target_type(elem)?;
                Some(self.type_ctx.interner.array(elem))
            }
            None => self.type_ctx.resolved_type(name),
        }
    }

    /// Pass: register every `extend Type { ... }` block's methods. Extension methods are lowered
    /// exactly like struct methods (`{target}_{method}` + implicit `this`) but the target's
    /// runtime representation is untouched (it is NOT added to the struct table), so primitives
    /// keep their value/reference semantics.
    pub(in crate::analyzer) fn register_extensions(
        &mut self,
        node: &'a ProgramView<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        for ext in node.extends.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(ext.file_path.as_deref()));
            diagnostics.file_path = file_path_string(&ext.file_path);
            let target = ext.target.text.clone();
            // `sealed` types reject user-authored `extend` blocks. Compiler-synthesized extends
            // (interface defaults, `@json` converters) are exempt, so a sealed type may still
            // implement interfaces with default methods or derive `@json`.
            let target_def = self
                .type_ctx
                .nominal_kind(&target)
                .and_then(|kind| self.type_ctx.resolve(kind, &target));
            if !ext.is_synthesized && target_def.is_some_and(|def| self.sealed_types.contains(&def))
            {
                diagnostics.report_error(
                    format!("Cannot extend sealed type '{}'", target),
                    Some(ext.target.position),
                );
                continue;
            }
            if self.is_static_class_name(&target) {
                for method in &ext.methods {
                    if !method.is_static {
                        diagnostics.report_error(
                            format!(
                                "extend of static class '{}' can only add static members ({} is not static)",
                                target, method.name.text
                            ),
                            Some(method.name.position),
                        );
                    }
                }
            }
            if ext.generic_parameters.is_some() {
                // Generic extend blocks were stashed by `stash_generic_extensions` and are attached
                // per instantiation in `ensure_*_instantiated`; here we only validate the target is
                // a known generic union, struct, interface, or the `T[]` array template.
                let target = &ext.target.text;
                if target.ends_with("[]") {
                    continue;
                }
                if self.type_ctx.resolve(DefKind::Union, target).is_none()
                    && self.type_ctx.resolve(DefKind::Struct, target).is_none()
                    && self.type_ctx.resolve(DefKind::Interface, target).is_none()
                {
                    diagnostics.report_error(
                        format!(
                            "Cannot extend unknown generic type '{}' (no generic union, class, or interface by that name)",
                            target
                        ),
                        Some(ext.target.position),
                    );
                }
                continue;
            }
            if !self.is_extendable_target(&target) {
                diagnostics.report_error(
                    format!("Cannot extend unknown type '{}'", target),
                    Some(ext.target.position),
                );
                continue;
            }
            let Some(target_type) = self.extend_target_type(&target) else { continue; };
            self.register_methods_for(target_type, &ext.methods, &GenericBindings::new(), diagnostics);
            // An `extend Type : Iface { ... }` block records that its target implements the
            // interface(s), so the target (including a primitive like `int`) participates in
            // interface dispatch and satisfies generic constraints (`T : Comparable<T>`). The
            // block's own methods supply the required signatures.
            if !ext.implements.is_empty() {
                self.validate_implements(
                    target_type,
                    &ext.implements,
                    &ext.methods,
                    &GenericBindings::new(),
                    ext.target.position,
                    diagnostics,
                );
            }
        }
    }

    /// Pre-pass: stash every generic `extend Type<...> { ... }` block keyed by its target type
    /// name, so the methods are available to monomorphize at the first instantiation of that type
    /// (which can happen as early as `register_enums`). Validation of the target is deferred to
    /// `register_extensions`, once all type templates are registered.
    ///
    /// Generic array templates (`extend T[] { … }`) are keyed under [`ARRAY_EXTEND_KEY`] (`"[]"`),
    /// not under the spelling `T[]`, so every concrete `Elem[]` shares one template.
    pub(in crate::analyzer) fn stash_generic_extensions(&mut self, node: &'a ProgramView<'a>) {
        use super::super::GenericExtendTarget;
        for ext in node.extends.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(ext.file_path.as_deref()));
            if ext.generic_parameters.is_some() {
                let key = if ext.target.text.ends_with("[]") {
                    GenericExtendTarget::Array
                } else {
                    let Some(def) = self.type_ctx.nominal_kind(&ext.target.text).and_then(|kind| self.type_ctx.resolve(kind, &ext.target.text)) else { continue; };
                    GenericExtendTarget::Nominal(def)
                };
                self.generic_extends.entry(key).or_default().push(ext);
            }
        }
    }

    /// Validates an `override` object-protocol method: `override` may only mark `to_string`
    /// / `hash_code`, those must be exported with the exact protocol signature, and a method
    /// that shadows a protocol name must carry `override`.
    pub(in crate::analyzer) fn validate_protocol_override(
        &self,
        method: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let name = method.name.text.as_str();

        // Constructors/destructors: `del` takes no parameters and neither declares a return type.
        if name == dream_syntax::nodes::types::DESTRUCTOR_NAME && !method.parameters.is_empty() {
            diagnostics.report_error(
                "destructor 'del' must not declare parameters".to_string(),
                Some(method.name.position),
            );
        }
        if dream_syntax::nodes::types::is_special_member_name(name) && method.return_type.is_some()
        {
            diagnostics.report_error(
                format!("'{}' must not declare a return type", name),
                Some(method.name.position),
            );
        }

        let is_protocol =
            name == dream_abi::intrinsics::TO_STRING || name == dream_abi::intrinsics::HASH_CODE;

        let is_override = method.is_override;

        if is_override && !is_protocol {
            diagnostics.report_error(
                format!("'override' can only be applied to object-protocol methods (to_string, hash_code), not '{}'", name),
                Some(method.name.position),
            );
            return;
        }

        if is_protocol && !is_override {
            diagnostics.report_error(
                format!(
                    "method '{}' overrides an object-protocol method; mark it with 'override'",
                    name
                ),
                Some(method.name.position),
            );
            return;
        }

        if is_override && is_protocol {
            if !method.visibility.is_public() {
                diagnostics.report_error(
                    format!(
                        "overridden object-protocol method '{}' must be declared 'public'",
                        name
                    ),
                    Some(method.name.position),
                );
            }
            if !method.parameters.is_empty() {
                diagnostics.report_error(
                    format!(
                        "overridden object-protocol method '{}' must not declare parameters",
                        name
                    ),
                    Some(method.name.position),
                );
            }
            let return_type = method.return_type.as_ref().map(|t| t.get_type());
            let expected = if name == "to_string" { "string" } else { "int" };
            if return_type.as_deref() != Some(expected) {
                diagnostics.report_error(
                    format!("overridden '{}' must return '{}'", name, expected),
                    Some(method.name.position),
                );
            }
        }
    }

    /// Validates a TypeScript-style property accessor (`get`/`set`): a getter takes no parameters
    /// and returns a non-`void` value; a setter takes exactly one parameter; neither may be `static`
    /// or `async`. Non-accessor methods are ignored.
    pub(in crate::analyzer) fn validate_accessor(
        &self,
        method: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let Some(kind) = method.accessor else {
            return;
        };
        let prop = &method.name.text;
        // Static accessors are permitted: `static get`/`static set` are read/written through the
        // type (`Type.prop` / `Type.prop = v`) with no `this`. `async` accessors are not: a getter
        // read must yield the property value directly, not a `Future`.
        if method.is_async {
            diagnostics.report_error(
                format!("property accessor '{}' cannot be 'async'", prop),
                Some(method.name.position),
            );
        }
        match kind {
            dream_syntax::nodes::function::AccessorKind::Get => {
                if !method.parameters.is_empty() {
                    diagnostics.report_error(
                        format!("getter '{}' must not declare parameters", prop),
                        Some(method.name.position),
                    );
                }
                if matches!(method.return_type, None | Some(Type::Void)) {
                    diagnostics.report_error(
                        format!("getter '{}' must declare a non-void return type", prop),
                        Some(method.name.position),
                    );
                }
            }
            dream_syntax::nodes::function::AccessorKind::Set => {
                if method.parameters.len() != 1 {
                    diagnostics.report_error(
                        format!("setter '{}' must declare exactly one parameter", prop),
                        Some(method.name.position),
                    );
                }
            }
        }
    }
}
