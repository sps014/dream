//! Interface declarations, monomorphization, and implementation validation: registering interface
//! defs + method slots, instantiating generic interface templates, building the runtime interface
//! table, the interface-membership/assignability queries, and `validate_implements` (checking a
//! class satisfies each interface it names). These are `impl Analyzer` methods, kept in the
//! `declarations` module alongside the other top-level registration passes.

use super::*;
use crate::module_graph::ProgramView;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{FunctionNode, Type};
use indexmap::IndexMap as HashMap;

impl<'a> Analyzer<'a> {
    /// Pass: register every interface's `DefId` and its method signatures. Interfaces declare method
    /// signatures (no fields in v1); a method may carry a default body that implementers inherit (see
    /// `driver::interface_defaults`). Generic interfaces are stashed as templates and monomorphized on
    /// demand. The declaration order of methods is their local index (used later for itable slots).
    ///
    /// Interfaces may extend parents (`interface Child : Parent + Other`). Parent relationships are
    /// recorded here; method lists for non-generic interfaces are flattened in
    /// [`finalize_interface_inheritance`] after every interface name is known. Generic instances
    /// flatten inside [`ensure_interface_instantiated`].
    pub(in crate::analyzer) fn register_interfaces(
        &mut self,
        node: &'a ProgramView<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        for iface in node.interfaces.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(iface.file_path.as_deref()));
            diagnostics.file_path = file_path_string(&iface.file_path);
            let def = self.type_ctx.register(
                DefKind::Interface,
                &iface.name.text,
                generic_param_names(&iface.generic_parameters),
            );
            self.type_visibility
                .insert(def, (iface.file_path.clone(), iface.visibility));
            for method in iface.methods.iter() {
                if method.is_static {
                    diagnostics.report_error(
                        format!(
                            "Interface method '{}' cannot be 'static' (interface methods are dynamically dispatched instance methods)",
                            method.name.text
                        ),
                        Some(method.name.position),
                    );
                }
            }

            if self.interface_decls.insert(def, iface).is_some() {
                diagnostics.report_error(
                    format!("Interface '{}' is already defined", iface.name.text),
                    Some(iface.name.position),
                );
            }
            self.interface_parents.insert(def, iface.parents.clone());

            if iface.generic_parameters.is_some() {
                if self.generic_interfaces.insert(def, iface).is_some() {
                    diagnostics.report_error(
                        format!("Interface '{}' is already defined", iface.name.text),
                        Some(iface.name.position),
                    );
                }
                continue;
            }

            // Own methods only for now; [`finalize_interface_inheritance`] merges parents.
            let methods: Vec<&'a FunctionNode<'a>> =
                iface.methods.iter().filter(|m| !m.is_static).collect();
            let ty = self.type_ctx.interner.interface_ty(def, vec![]);
            if self.interface_methods.insert(ty, methods).is_some() {
                diagnostics.report_error(
                    format!("Interface '{}' is already defined", iface.name.text),
                    Some(iface.name.position),
                );
            }
        }
        self.finalize_interface_inheritance(diagnostics);
    }

    /// After every interface name is registered, flatten non-generic interfaces that extend parents
    /// so their `interface_methods` entries include the inherited closure (and diagnose cycles /
    /// ambiguous defaults).
    fn finalize_interface_inheritance(&mut self, diagnostics: &mut DiagnosticBag) {
        let defs: Vec<_> = self
            .interface_decls
            .iter()
            .filter(|(_, decl)| decl.generic_parameters.is_none())
            .map(|(&def, _)| def)
            .collect();
        for def in defs {
            let ty = self.type_ctx.instantiate(def, vec![]);
            let _ = self.flatten_interface_methods(ty, diagnostics, &mut Vec::new());
        }
    }

    pub(in crate::analyzer) fn ensure_interface_instantiated(
        &mut self,
        base_name: &str,
        args: &[Type],
        position: &TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        let Some(def) = self.type_ctx.resolve(DefKind::Interface, base_name) else {
            return;
        };
        let ids = args.iter().map(|arg| self.type_ctx.lower(arg)).collect();
        let ty = self.type_ctx.instantiate(def, ids);
        if let Some(template) = self.interface_decl(def) {
            Self::check_generic_arity(
                "interface",
                base_name,
                template.generic_parameters.as_deref().unwrap_or(&[]).len(),
                args.len(),
                position,
                diagnostics,
            );
        }
        let _ = self.flatten_interface_methods(ty, diagnostics, &mut Vec::new());
        if self.interface_extensions_attached.insert(ty) {
            let args = match self.type_ctx.interner.kind(ty) {
                dream_types::TyKind::Interface(_, args) => args
                    .iter()
                    .map(|&arg| self.type_ctx.syntax_type(arg))
                    .collect::<Vec<_>>(),
                _ => return,
            };
            self.register_generic_extension_methods(
                GenericExtendTarget::Nominal(def),
                ty,
                &args,
                diagnostics,
            );
        }
    }

    /// Tracks template definitions rather than instances so expanding generic inheritance cycles
    /// are rejected even when each recursive step introduces different type arguments.
    fn flatten_interface_methods(
        &mut self,
        key: dream_types::TypeId,
        diagnostics: &mut DiagnosticBag,
        stack: &mut Vec<dream_types::DefId>,
    ) -> Option<dream_types::TypeId> {
        let dream_types::TyKind::Interface(def, ids) = self.type_ctx.interner.kind(key).clone()
        else {
            return None;
        };
        let template = *self.interface_decl(def)?;
        let base_name = template.name.text.clone();
        if stack.contains(&def) {
            let names = stack
                .iter()
                .chain(std::iter::once(&def))
                .map(|&id| self.type_ctx.defs.name(id))
                .collect::<Vec<_>>()
                .join(" -> ");
            diagnostics.report_error(
                format!("interface inheritance cycle involving '{}'", names),
                Some(template.name.position),
            );
            return None;
        }
        if self.interface_parent_instances.contains_key(&key) {
            return Some(key);
        }
        let scope = self.type_ctx.scope();
        self.type_ctx.set_scope(def.module);
        let args: Vec<_> = ids
            .iter()
            .map(|&id| self.type_ctx.syntax_type(id))
            .collect();
        let params = template.generic_parameters.as_deref().unwrap_or(&[]);
        let bindings = generic_bindings(params, &args);
        stack.push(def);

        let mut merged: Vec<&'a FunctionNode<'a>> = Vec::new();
        let mut from_parent: HashMap<String, (bool, dream_types::TypeId)> = HashMap::new();
        let mut parent_keys: Vec<dream_types::TypeId> = Vec::new();

        let parents = template.parents.clone();
        for parent_ty in &parents {
            let Some((pbase, pargs_raw)) = Self::resolve_struct_parts(parent_ty) else {
                diagnostics.report_error(
                    format!(
                        "interface '{}' parent must be an interface type, got {}",
                        base_name,
                        self.ty_display(parent_ty)
                    ),
                    parent_ty.get_span().or(Some(template.name.position)),
                );
                continue;
            };
            let Some(parent_def) = self.type_ctx.resolve(DefKind::Interface, &pbase) else {
                diagnostics.report_error(
                    format!(
                        "interface '{}' cannot extend '{}': not an interface",
                        base_name, pbase
                    ),
                    parent_ty.get_span().or(Some(template.name.position)),
                );
                continue;
            };
            let pargs: Vec<Type> = pargs_raw
                .iter()
                .map(|t| substitute_generic_type(t, &bindings))
                .collect();
            let arg_ids = pargs.iter().map(|arg| self.type_ctx.lower(arg)).collect();
            let parent = self.type_ctx.instantiate(parent_def, arg_ids);
            let Some(parent_key) = self.flatten_interface_methods(parent, diagnostics, stack)
            else {
                continue;
            };
            self.type_ctx.set_scope(def.module);
            if !parent_keys.contains(&parent_key) {
                parent_keys.push(parent_key);
            }
            let parent_methods = self
                .interface_method_list(parent_key)
                .cloned()
                .unwrap_or_default();
            for pm in parent_methods {
                let name = accessor_member_name(pm);
                if let Some((prev_default, prev_src)) = from_parent.get(&name) {
                    if *prev_default && pm.is_default_impl {
                        diagnostics.report_error(
                            format!(
                                "interface '{}': ambiguous default for method '{}' inherited from both '{}' and '{}'; override it on '{}'",
                                base_name, name, dream_types::display_name(&self.type_ctx.interner, &self.type_ctx.defs, *prev_src), dream_types::display_name(&self.type_ctx.interner, &self.type_ctx.defs, parent_key), base_name
                            ),
                            Some(template.name.position),
                        );
                    }
                    continue;
                }
                from_parent.insert(name, (pm.is_default_impl, parent_key));
                merged.push(pm);
            }
        }

        let own: Vec<&'a FunctionNode<'a>> = {
            let mut owned = Vec::new();
            for method in template.methods.iter().filter(|m| !m.is_static) {
                let mut m = method.clone();
                Self::substitute_generic_signature(&mut m, &bindings);
                for parameter in &mut m.parameters {
                    let id = self.type_ctx.lower(&parameter.type_);
                    parameter.type_ = self.type_ctx.syntax_type(id);
                }
                if let Some(ret) = &m.return_type {
                    let id = self.type_ctx.lower(ret);
                    m.return_type = Some(self.type_ctx.syntax_type(id));
                }
                let method_ref: &'a FunctionNode<'a> = self.arena.alloc(m);
                owned.push(method_ref);
            }
            owned
        };

        for om in own {
            if let Some(pos) = merged
                .iter()
                .position(|m| accessor_member_name(m) == accessor_member_name(om))
            {
                merged[pos] = om;
            } else {
                merged.push(om);
            }
        }

        stack.pop();
        self.interface_parent_instances.insert(key, parent_keys);
        self.interface_methods.insert(key, merged);
        self.type_ctx.set_scope(scope);
        Some(key)
    }

    pub(in crate::analyzer) fn collect_interface_ancestors(
        &self,
        ty: dream_types::TypeId,
        out: &mut Vec<dream_types::TypeId>,
    ) {
        if out.contains(&ty) {
            return;
        }
        out.push(ty);
        if let Some(parents) = self.interface_parent_instances.get(&ty) {
            for &parent in parents {
                self.collect_interface_ancestors(parent, out);
            }
        }
    }

    /// Builds the interface dispatch metadata carried into codegen: the ordered interfaces (index =
    /// `iface_id`) with each method slot's `call_indirect` signature, and, per implementing class,
    /// the concrete method symbol filling each `(interface, slot)`.
    pub(in crate::analyzer) fn hir_build_interfaces(&mut self) -> dream_hir::InterfaceTable {
        use dream_hir::{InterfaceImpl, InterfaceInfo, InterfaceTable};

        let iface_order: Vec<(dream_types::TypeId, Vec<&'a FunctionNode<'a>>)> = self
            .interface_methods
            .iter()
            .map(|(ty, methods)| (*ty, methods.clone()))
            .collect();

        let mut name_to_id: HashMap<dream_types::TypeId, usize> = HashMap::new();
        let mut interfaces = Vec::with_capacity(iface_order.len());
        for (id, (name, methods)) in iface_order.iter().enumerate() {
            name_to_id.insert(*name, id);
            let sigs: Vec<dream_types::TypeId> = methods
                .iter()
                .map(|m| self.interface_dispatch_sig(m))
                .collect();
            interfaces.push(InterfaceInfo {
                ty: *name,
                name: dream_types::display_name(
                    &self.type_ctx.interner,
                    &self.type_ctx.defs,
                    *name,
                ),
                method_count: methods.len(),
                sigs,
            });
        }

        let mut class_impls: Vec<(dream_types::TypeId, Vec<dream_types::TypeId>)> = self
            .implements
            .iter()
            .map(|(class, ifaces)| (*class, ifaces.clone()))
            .collect();
        class_impls.sort();
        let mut impls = Vec::new();
        for (class_ty, ifaces) in class_impls {
            if let dream_types::TyKind::Struct(def, _) | dream_types::TyKind::Union(def, _) =
                self.type_ctx.interner.kind(class_ty)
            {
                self.type_ctx.set_scope(def.module);
            }
            let mut entries = Vec::new();
            for iface in ifaces {
                let Some(&id) = name_to_id.get(&iface) else {
                    continue;
                };
                let methods = self
                    .interface_methods
                    .get(&iface)
                    .cloned()
                    .unwrap_or_default();
                let definitions = methods
                    .iter()
                    .map(|m| {
                        self.function_table
                            .methods
                            .get(&(class_ty, accessor_member_name(m)))
                            .and_then(|methods| methods.first())
                            .map(|function| function.0)
                    })
                    .collect();
                entries.push((id, definitions));
            }
            impls.push(InterfaceImpl { class_ty, entries });
        }

        InterfaceTable { interfaces, impls }
    }

    pub(in crate::analyzer) fn is_interface_name(&self, ty: dream_types::TypeId) -> bool {
        matches!(
            self.type_ctx.interner.kind(ty),
            dream_types::TyKind::Interface(_, _)
        )
    }
    pub(in crate::analyzer) fn is_generic_interface(&self, def: dream_types::DefId) -> bool {
        self.generic_interface(def).is_some()
    }
    pub(in crate::analyzer) fn class_implements(
        &self,
        class: dream_types::TypeId,
        iface: dream_types::TypeId,
    ) -> bool {
        self.implemented_interfaces(class)
            .is_some_and(|ifaces| ifaces.contains(&iface))
    }
    pub(in crate::analyzer) fn implements_as_interface_ref(
        &mut self,
        class: dream_types::TypeId,
        iface: dream_types::TypeId,
        diagnostics: &mut DiagnosticBag,
    ) -> bool {
        if matches!(
            self.type_ctx.interner.kind(class),
            dream_types::TyKind::Array(_)
        ) {
            self.ensure_array_collection(class, diagnostics);
        }
        self.class_implements(class, iface)
    }

    /// True when `iface_method` and `class_method` have matching signatures (same parameter types
    /// in order, matching return types, and the same async-ness). Return types may be an exact
    /// match or a class return that is assignable to the interface return (e.g. a concrete
    /// `ListIterator<T>` for an `Iterator<T>` interface requirement). An `async` interface method
    /// must be implemented by an `async` method (and vice versa) because the two dispatch to
    /// different code shapes (a `Future`-producing constructor vs. a plain call).
    fn interface_method_matches(
        &mut self,
        iface: &FunctionNode,
        class: &FunctionNode,
        bindings: &GenericBindings,
        diagnostics: &mut DiagnosticBag,
    ) -> bool {
        if iface.accessor != class.accessor
            || iface.is_async != class.is_async
            || iface.parameters.len() != class.parameters.len()
        {
            return false;
        }
        let scope = self.type_ctx.scope();
        for (a, b) in iface.parameters.iter().zip(&class.parameters) {
            self.type_ctx
                .set_scope(self.graph.module_for_file(iface.file_path.as_deref()));
            let a = self.type_ctx.lower(&a.type_);
            self.type_ctx
                .set_scope(self.graph.module_for_file(class.file_path.as_deref()));
            let b = self
                .type_ctx
                .lower(&substitute_generic_type(&b.type_, bindings));
            if a != b {
                self.type_ctx.set_scope(scope);
                return false;
            }
        }
        self.type_ctx
            .set_scope(self.graph.module_for_file(iface.file_path.as_deref()));
        let a = self
            .type_ctx
            .lower(&iface.return_type.clone().unwrap_or(Type::Void));
        self.type_ctx
            .set_scope(self.graph.module_for_file(class.file_path.as_deref()));
        let class_ret =
            substitute_generic_type(&class.return_type.clone().unwrap_or(Type::Void), bindings);
        if let Some((base, args)) = Self::resolve_struct_parts(&class_ret)
            && !args.is_empty() {
                let position = class.name.position;
                self.ensure_type_instantiated(&base, &args, &position, diagnostics);
            }
        let b = self.type_ctx.lower(&class_ret);
        self.type_ctx.set_scope(scope);
        self.value_type_assignable(a, b, diagnostics)
    }

    /// Validates a class's `implements` clause: every listed type must name an interface, and the
    /// class must provide an instance method with a matching signature for each interface method.
    /// Records the validated (mangled) interface list in `self.implements` under `class_name`.
    ///
    /// Works uniformly for non-generic classes (`bindings` empty) and monomorphized generic classes
    /// (`bindings` maps the class's type parameters to concrete types). For a monomorphized class,
    /// the `implements` entries are expected to already be substituted (e.g. `Container<int>`) while
    /// `methods` are the unsubstituted template methods, substituted here for signature comparison.
    /// Generic interfaces named in the clause are instantiated on demand.
    pub(in crate::analyzer) fn validate_implements(
        &mut self,
        class: dream_types::TypeId,
        implements: &[Type],
        methods: &[FunctionNode<'a>],
        bindings: &GenericBindings,
        class_pos: TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        if implements.is_empty() {
            return;
        }
        let class_name =
            dream_types::display_name(&self.type_ctx.interner, &self.type_ctx.defs, class);
        let mut validated = Vec::new();
        for iface_ty in implements {
            let span = iface_ty.get_span().unwrap_or(class_pos);
            let (base, args) = match Self::resolve_struct_parts(iface_ty) {
                Some(parts) => parts,
                None => continue,
            };
            let iface = self.type_ctx.lower(iface_ty);
            if !self.is_interface_name(iface) {
                diagnostics.report_error(
                    format!(
                        "'{}' is not an interface (class '{}' can only implement interfaces)",
                        base, class_name
                    ),
                    Some(span),
                );
                continue;
            }
            self.ensure_interface_instantiated(&base, &args, &span, diagnostics);
            let iface_name =
                dream_types::display_name(&self.type_ctx.interner, &self.type_ctx.defs, iface);
            let iface_methods = match self.interface_method_list(iface) {
                Some(methods) => methods.clone(),
                None => continue,
            };
            for im in &iface_methods {
                let im_key = accessor_member_name(im);
                match methods
                    .iter()
                    .find(|cm| accessor_member_name(cm) == im_key && !cm.is_static)
                {
                    Some(cm) => {
                        let matches = if bindings.is_empty() {
                            self.interface_method_matches(im, cm, bindings, diagnostics)
                        } else {
                            let mut sub = cm.clone();
                            Self::substitute_generic_signature(&mut sub, bindings);
                            self.interface_method_matches(im, &sub, bindings, diagnostics)
                        };
                        if !matches {
                            diagnostics.report_error(
                                format!(
                                    "class '{}' method '{}' does not match the signature required by interface '{}'",
                                    class_name, im.name.text, iface_name
                                ),
                                Some(cm.name.position),
                            );
                        }
                    }
                    None if im.is_default_impl => {
                        // Satisfied by the interface's default body, which is injected as an
                        // `extend <class> { ... }` method before analysis (see
                        // `generate_interface_default_impls`), so the class need not declare it.
                    }
                    None => {
                        diagnostics.report_error(
                            format!(
                                "class '{}' does not implement method '{}' required by interface '{}'",
                                class_name, im.name.text, iface_name
                            ),
                            Some(class_pos),
                        );
                    }
                }
            }
            if !validated.contains(&iface) {
                // Explicit implement plus every parent interface (subtype relationship).
                let mut ancestors = Vec::new();
                self.collect_interface_ancestors(iface, &mut ancestors);
                for a in ancestors {
                    if !validated.contains(&a) {
                        validated.push(a);
                    }
                }
            }
            // Attach `extend Collection<T>`-style package methods onto this class so
            // `list.to_list()` resolves without going through the interface receiver.
            self.attach_interface_extension_methods(iface, class, diagnostics);
        }
        // Merge into any interfaces already recorded for this type (a class may gain further
        // interfaces through an `extend : Iface` block) rather than replacing them.
        let entry = self.implements.entry(class).or_default();
        for iface in validated {
            if !entry.contains(&iface) {
                entry.push(iface);
            }
        }
    }

    /// Registers package `extend Iface<…>` methods onto `target` (a concrete class or interface
    /// instance name), walking parent interfaces so `extend Collection<T>` applies when the class
    /// only declares `IndexedCollection<T>`.
    fn attach_interface_extension_methods(
        &mut self,
        iface: dream_types::TypeId,
        target: dream_types::TypeId,
        diagnostics: &mut DiagnosticBag,
    ) {
        let mut ancestors = Vec::new();
        self.collect_interface_ancestors(iface, &mut ancestors);
        for iface in ancestors {
            let dream_types::TyKind::Interface(def, args) =
                self.type_ctx.interner.kind(iface).clone()
            else {
                continue;
            };
            let args: Vec<_> = args
                .iter()
                .map(|&arg| self.type_ctx.syntax_type(arg))
                .collect();
            self.register_generic_extension_methods(
                GenericExtendTarget::Nominal(def),
                target,
                &args,
                diagnostics,
            );
        }
    }
}
