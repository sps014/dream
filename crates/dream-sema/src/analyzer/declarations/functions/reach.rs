//! Demand-driven analysis of library bodies. Every program merges the whole standard prelude, but
//! most of its bodies are unreachable from user code, and analyzing them dominated small builds.
//! With [`Analyzer::with_lazy_library_bodies`], library bodies wait in `deferred_bodies` until HIR
//! that is already emitted reaches them.
//!
//! The edges are a superset of what MIR dead-code pruning keeps, because anything this pass skips
//! has no body for the backend: direct callees and function references; the destructor and object
//! protocol methods of every type a live body mentions, nested through type arguments and fields;
//! the `to_string`/`hash_code` overrides of every primitive; and, for each interface slot called,
//! the method that fills it in every implementing type.
//!
//! Skipped bodies also no longer instantiate the generic value structs they name, yet every
//! registered struct, union, and tuple gets a layout, and a value-typed field needs its own. Those
//! field types are instantiated here instead.

use super::*;
use dream_hir::{HirEdges, hir_body_edges, hir_expr_edges};
use dream_syntax::nodes::types::DESTRUCTOR_NAME;
use dream_types::{DefId, TyKind, TypeId};

#[derive(Default)]
pub(in crate::analyzer) struct LibraryReach {
    scanned_functions: usize,
    scanned_structs: usize,
    scanned_unions: usize,
    scanned_types: usize,
    roots_scanned: bool,
    live_types: HashSet<TypeId>,
    /// Live generic instances no body has instantiated yet; their members do not exist until then.
    uninstantiated: HashSet<TypeId>,
    interface_slots: HashSet<(usize, usize)>,
}

impl<'a> Analyzer<'a> {
    /// Queues a library body instead of analyzing it now. Exported functions stay eager because
    /// they are entry points of their own.
    pub(in crate::analyzer) fn defer_library_body(
        &mut self,
        function: &'a FunctionNode<'a>,
        bindings: &GenericBindings,
    ) -> bool {
        if !self.lazy_library_bodies
            || !self.is_library_file(function.file_path.as_deref())
            || dream_abi::attributes::has_export_attr(&function.attributes)
        {
            return false;
        }
        let Some((def, _)) = self.function_table.declaration_node(function) else {
            return false;
        };
        self.deferred_bodies
            .insert(def, (function, bindings.clone()));
        true
    }

    /// Analyzes every deferred body reachable from the HIR emitted since the previous call.
    /// Returns whether anything was analyzed, so the instantiation fixpoint scans the new bodies
    /// after the struct methods they instantiate have been registered.
    pub(in crate::analyzer) fn analyze_reachable_library_bodies(
        &mut self,
        reach: &mut LibraryReach,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<bool, SemanticError> {
        if !self.lazy_library_bodies {
            return Ok(false);
        }
        let mut progressed = self.instantiate_value_field_types(reach);
        let mut edges = HirEdges::default();
        if !reach.roots_scanned {
            reach.roots_scanned = true;
            for global in &self.hir.global_decls {
                edges.types.push(global.ty);
                if let Some(init) = &global.init {
                    hir_expr_edges(init, &mut edges);
                }
            }
            edges.types.extend(
                self.type_ctx
                    .interner
                    .iter_kinds()
                    .filter(|(_, kind)| matches!(kind, TyKind::Prim(_)))
                    .map(|(id, _)| id),
            );
        }
        for function in &self.hir.functions[reach.scanned_functions..] {
            hir_body_edges(&function.body, &mut edges);
            edges.types.push(function.ret);
            edges.types.extend(function.instance.iter().copied());
            edges.types.extend(function.params.iter().map(|p| p.ty));
            edges.types.extend(function.locals.iter().map(|l| l.ty));
        }
        reach.scanned_functions = self.hir.functions.len();

        let mut demanded: Vec<DefId> = Vec::new();
        let mut types = edges.types;
        types.append(&mut edges.expr_types);
        for (def, instance) in edges.callees {
            demanded.push(def);
            types.extend(instance);
        }
        for ty in std::mem::take(&mut reach.uninstantiated) {
            self.member_edges(ty, reach, &mut types, &mut demanded);
        }
        while let Some(ty) = types.pop() {
            if reach.live_types.insert(ty) {
                self.component_types(ty, &mut types);
                self.member_edges(ty, reach, &mut types, &mut demanded);
            }
        }
        reach.interface_slots.extend(edges.iface_calls);
        for &(iface_id, slot) in &reach.interface_slots {
            self.interface_slot_impls(iface_id, slot, &mut demanded);
        }

        // A body with errors emits no HIR, so its library callees would never be reached and
        // their own diagnostics would go missing.
        if diagnostics.has_errors() {
            demanded.extend(self.deferred_bodies.keys().copied());
        }
        for def in demanded {
            let Some((function, bindings)) = self.deferred_bodies.shift_remove(&def) else {
                continue;
            };
            diagnostics.file_path = file_path_string(&function.file_path);
            self.with_generic_bindings(bindings, |s| s.analyze_function(function, diagnostics))?;
            progressed = true;
        }
        Ok(progressed)
    }

    /// Returns whether any struct was instantiated, since its methods still need a decision.
    fn instantiate_value_field_types(&mut self, reach: &mut LibraryReach) -> bool {
        let structs = self.struct_table.structs.len();
        let mut fields: Vec<TypeId> = self.struct_table.structs[reach.scanned_structs..]
            .values()
            .flat_map(|info| info.fields.values().map(|f| f.ty))
            .collect();
        fields.extend(
            self.union_table[reach.scanned_unions..]
                .values()
                .flat_map(|info| {
                    info.variants
                        .iter()
                        .flat_map(|v| v.fields.iter().map(|f| f.ty))
                }),
        );
        fields.extend(
            self.type_ctx
                .interner
                .iter_kinds()
                .skip(reach.scanned_types)
                .filter(|(_, kind)| matches!(kind, TyKind::Tuple(_)))
                .map(|(id, _)| id),
        );
        reach.scanned_structs = structs;
        reach.scanned_unions = self.union_table.len();
        reach.scanned_types = self.type_ctx.interner.len();
        let mut seen = HashSet::new();
        for ty in fields {
            self.instantiate_value_fields(ty, &mut seen);
        }
        self.struct_table.structs.len() != structs
    }

    fn component_types(&self, ty: TypeId, types: &mut Vec<TypeId>) {
        match self.type_ctx.interner.kind(ty) {
            TyKind::Array(elem) => types.push(*elem),
            TyKind::Struct(_, args)
            | TyKind::Union(_, args)
            | TyKind::Interface(_, args)
            | TyKind::Tuple(args) => types.extend(args.iter().copied()),
            TyKind::Func(params, ret) => {
                types.extend(params.iter().copied());
                types.push(*ret);
            }
            TyKind::Prim(_)
            | TyKind::Object
            | TyKind::Void
            | TyKind::Error
            | TyKind::Enum(_)
            | TyKind::Js => {}
        }
    }

    /// Fields, destructor, and object protocol methods. A generic instance no body has
    /// instantiated yet has none of these, so it is retried every round until it exists.
    fn member_edges(
        &self,
        ty: TypeId,
        reach: &mut LibraryReach,
        types: &mut Vec<TypeId>,
        demanded: &mut Vec<DefId>,
    ) {
        let uninstantiated = match self.type_ctx.interner.kind(ty) {
            TyKind::Struct(_, args) => {
                !args.is_empty() && self.struct_table.get_struct(ty).is_none()
            }
            TyKind::Union(_, args) => !args.is_empty() && !self.union_table.contains_key(&ty),
            _ => false,
        };
        if uninstantiated {
            reach.uninstantiated.insert(ty);
            return;
        }
        if let Some(info) = self.struct_table.get_struct(ty) {
            types.extend(info.fields.values().map(|f| f.ty));
            if info.has_destructor
                && let Ok(destructor) = self.method_info(ty, DESTRUCTOR_NAME)
            {
                demanded.push(destructor.identity.0);
            }
        }
        if let Some(info) = self.union_table.get(&ty) {
            types.extend(
                info.variants
                    .iter()
                    .flat_map(|v| v.fields.iter().map(|f| f.ty)),
            );
        }
        demanded.extend(self.unique_method_def(ty, dream_abi::intrinsics::TO_STRING));
        demanded.extend(self.unique_method_def(ty, dream_abi::intrinsics::HASH_CODE));
    }

    /// Mirrors the interface table the backend dispatches through (`hir_build_interfaces`).
    fn interface_slot_impls(&self, iface_id: usize, slot: usize, demanded: &mut Vec<DefId>) {
        let Some((iface, methods)) = self.interface_methods.get_index(iface_id) else {
            return;
        };
        let Some(method) = methods.get(slot) else {
            return;
        };
        let member = accessor_member_name(method);
        for (class, ifaces) in &self.implements {
            if ifaces.contains(iface)
                && let Some(function) = self
                    .function_table
                    .methods
                    .get(&(*class, member.clone()))
                    .and_then(|methods| methods.first())
            {
                demanded.push(function.0);
            }
        }
    }
}
