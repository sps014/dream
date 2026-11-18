//! Persisted typed bodies of non-generic functions and methods.
//!
//! A body is stored only when it reported nothing and every program-wide change its analysis
//! made is accounted for by the [`journal`]: the types it interned and the generic types and
//! functions it asked to instantiate, in order. Compilation-local ids are stored as [`keys`].
//!
//! A later compilation repeats the journal at the same point of analysis, then reuses the body
//! when every key resolves, every generic function instance it calls has been requested, and
//! every instance that was registered when it was stored is registered again. Fresh analysis
//! would make the same requests and leave the same tables, so skipping it cannot alter later
//! analysis. When a check fails the body is analyzed afresh; the repeated requests are idempotent.
//! The cache key covers the module's content and its dependencies' interfaces, which fix every
//! resolution the body made.

mod journal;
mod keys;

use super::Analyzer;
use crate::errors::SemanticError;
use dream_diagnostics::DiagnosticBag;
use dream_hir::{GlobalId, HFunction, HirEdges, hir_body_edges};
use dream_syntax::nodes::{FunctionNode, StatementNode};
use dream_types::portable::{self, IdSpace, IdTables};
use dream_types::{DefId, ModuleId, TyKind, TypeId};
use indexmap::{IndexMap, IndexSet};
pub(super) use journal::{Journal, Request};
pub(super) use keys::DefIndex;
use keys::TypeKey;

/// Source file, name offset and name of a declaration.
pub type BodyKey = (String, usize, String);
pub type TypedBodies = IndexMap<BodyKey, CachedBody>;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CachedBody {
    hir: String,
    types: Vec<TypeKey>,
    defs: Vec<keys::DefKey>,
    globals: Vec<String>,
    interfaces: Vec<TypeKey>,
    registered: Vec<TypeKey>,
    events: Vec<journal::Event>,
}

impl<'a> Analyzer<'a> {
    /// Analyzes a non-generic body, replaying or recording its persisted HIR when possible.
    pub(in crate::analyzer) fn analyze_cacheable_body(
        &mut self,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let module = self.graph.module_for_file(function.file_path.as_deref());
        let key = self.typed_body_key(module, function, diagnostics);
        if let Some(key) = &key
            && let Some(hir) = self.reuse_body(module, key, function, diagnostics)
        {
            self.current_file = function.file_path.clone();
            self.type_ctx.set_scope(module);
            self.hir.functions.push(hir);
            self.reused_typed_bodies += 1;
            return Ok(());
        }

        let reported = diagnostics.diagnostics.len();
        let before = key.as_ref().map(|_| self.start_journal());
        let checked = self.cached_body_checks.contains(&module);
        let analyzed = self.analyze_function_with_body_checks(function, diagnostics, checked);
        let entries = before.and_then(|before| self.finish_journal(&before));
        analyzed?;
        if reported != diagnostics.diagnostics.len() {
            self.body_checks_with_diagnostics.insert(module);
        } else if let Some(key) = key
            && let Some(entries) = entries
            && let Some(body) = self.encode_body(&entries)
            && let Some(bodies) = self.cached_typed_bodies.get_mut(&module)
        {
            bodies.insert(key, body);
        }
        Ok(())
    }

    fn typed_body_key(
        &self,
        module: ModuleId,
        function: &FunctionNode<'_>,
        diagnostics: &DiagnosticBag,
    ) -> Option<BodyKey> {
        if diagnostics.has_errors()
            || self.journal.is_some()
            || !self.cached_typed_bodies.contains_key(&module)
            || !self.graph.consumed.is_empty()
            || self.generator_facts.is_some()
            || self.generator_scope.active()
            || !self.current_generic_bindings.is_empty()
            || function.is_extern
            || function.generic_parameters.is_some()
            || function.name.text == crate::entry::ENTRY_NAME
            || function.body.iter().any(StatementNode::contains_yield)
        {
            return None;
        }
        Some((
            function.file_path.as_ref()?.to_string(),
            function.name.position.start,
            function.name.text.clone(),
        ))
    }

    /// Sizes of the program-wide tables body analysis can grow. The emitted functions come last.
    fn effect_fingerprint(&self) -> Vec<usize> {
        let table = &self.function_table;
        vec![
            self.type_ctx.interner.len(),
            self.type_ctx.defs.len(),
            table.functions.len(),
            table.methods.len(),
            table.generic_methods.len(),
            table.generic_method_overloads.len(),
            self.struct_table.structs.len(),
            self.union_table.len(),
            self.enum_table.len(),
            self.generic_functions.len(),
            self.instantiated_generics.len(),
            self.pending_lambdas.len(),
            self.lambda_counter,
            self.closure_parents.len(),
            self.closure_captures.len(),
            self.closure_binding_modifiers.len(),
            self.direct_captures.len(),
            self.receiver_modes.len(),
            self.generic_structs.len(),
            self.generic_struct_instances.len(),
            self.c_wrapped_fun_params.len(),
            self.intrinsic_defs.len(),
            self.generic_method_bindings.len(),
            self.struct_methods.len(),
            self.deferred_bodies.len(),
            self.generic_unions.len(),
            self.generic_extends.len(),
            self.interface_methods.len(),
            self.generic_interfaces.len(),
            self.interface_parents.len(),
            self.interface_decls.len(),
            self.interface_parent_instances.len(),
            self.interface_extensions_attached.len(),
            self.array_collections_attached.len(),
            self.sealed_types.len(),
            self.type_visibility.len(),
            self.implements.len(),
            self.operator_overloads.len(),
            self.protocol_hooks.len(),
            self.globals.len(),
            self.hir.global_decls.len(),
            self.hir.functions.len(),
        ]
    }

    fn encode_body(&mut self, entries: &[journal::Entry]) -> Option<CachedBody> {
        self.refresh_def_index();
        let (hir, tables) = portable::encode(|| serde_json::to_string(self.hir.functions.last()?).ok());
        let hir = hir?;
        let types: Vec<_> = tables
            .get(IdSpace::Type)
            .iter()
            .map(|&raw| TypeId(raw as u32))
            .collect();
        let interfaces = tables
            .get(IdSpace::Interface)
            .iter()
            .map(|&raw| self.interface_methods.get_index(raw as usize).map(|(&ty, _)| ty))
            .collect::<Option<Vec<_>>>()?;
        let mut closure = IndexSet::new();
        for &ty in types.iter().chain(&interfaces) {
            self.collect_components(ty, &mut closure);
        }
        Some(CachedBody {
            hir,
            types: types.iter().map(|&ty| self.type_key(ty)).collect(),
            defs: tables
                .get(IdSpace::Def)
                .iter()
                .map(|&raw| self.def_key(DefId::from_raw(raw)))
                .collect(),
            globals: tables
                .get(IdSpace::Global)
                .iter()
                .map(|&raw| self.hir_global_name(GlobalId(raw as u32)).map(str::to_string))
                .collect::<Option<_>>()?,
            interfaces: interfaces.iter().map(|&ty| self.type_key(ty)).collect(),
            registered: closure
                .into_iter()
                .filter(|&ty| self.lazily_registered(ty))
                .map(|ty| self.type_key(ty))
                .collect(),
            events: self.encode_events(entries),
        })
    }

    fn reuse_body(
        &mut self,
        module: ModuleId,
        key: &BodyKey,
        function: &FunctionNode<'_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Option<HFunction> {
        let cached = self.cached_typed_bodies.get(&module)?.get(key)?.clone();
        self.current_file = function.file_path.clone();
        self.type_ctx.set_scope(module);
        self.replay_events(&cached.events, diagnostics)?;
        self.refresh_def_index();
        let mut tables = IdTables::default();
        tables.0[IdSpace::Type as usize] = cached
            .types
            .iter()
            .map(|key| self.resolve_type(key).map(|ty| ty.0 as u64))
            .collect::<Option<_>>()?;
        tables.0[IdSpace::Def as usize] = cached
            .defs
            .iter()
            .map(|key| self.resolve_def(key).map(DefId::to_raw))
            .collect::<Option<_>>()?;
        tables.0[IdSpace::Global as usize] = cached
            .globals
            .iter()
            .map(|name| self.hir_global_id(name).map(|id| id.0 as u64))
            .collect::<Option<_>>()?;
        tables.0[IdSpace::Interface as usize] = cached
            .interfaces
            .iter()
            .map(|key| {
                let ty = self.resolve_type(key)?;
                self.interface_index(ty).map(|index| index as u64)
            })
            .collect::<Option<_>>()?;
        for key in &cached.registered {
            if !self.lazily_registered(self.resolve_type(key)?) {
                return None;
            }
        }
        let hir: HFunction = portable::decode(tables, || serde_json::from_str(&cached.hir)).ok()?;
        let mut edges = HirEdges::default();
        hir_body_edges(&hir.body, &mut edges);
        let unrequested = edges.callees.iter().chain(&edges.funcrefs).any(|key| {
            !key.1.is_empty() && !self.instantiated_generics.contains_key(key)
        });
        if unrequested {
            return None;
        }
        self.rebind_cached_body(function, hir)
    }

    fn rebind_cached_body(
        &self,
        function: &FunctionNode<'_>,
        mut hir: HFunction,
    ) -> Option<HFunction> {
        let identity = self.function_table.declaration_node(function)?;
        let info = self.function_table.functions.get(&identity)?;
        if !identity.1.is_empty()
            || hir.def != identity.0
            || !hir.instance.is_empty()
            || hir.ret != info.resolved_return
            || hir.file.as_deref() != function.file_path.as_deref()
        {
            return None;
        }
        let lookup = self.function_table.emitted_name(&self.type_ctx, &identity);
        hir.symbol = self.hir_function_symbol(&lookup, function.file_path.as_ref(), &[]);
        hir.name = lookup;
        Some(hir)
    }

    /// Whether `ty` is a type whose members analysis registers on first use, and that use already
    /// happened.
    fn lazily_registered(&self, ty: TypeId) -> bool {
        match self.type_ctx.interner.kind(ty) {
            TyKind::Struct(_, args) if !args.is_empty() => self.struct_info(ty).is_some(),
            TyKind::Union(_, args) if !args.is_empty() => self.union_info(ty).is_some(),
            TyKind::Interface(..) => self.interface_extensions_attached.contains(&ty),
            TyKind::Array(_) => self.array_collections_attached.contains(&ty),
            _ => false,
        }
    }

    fn collect_components(&self, ty: TypeId, out: &mut IndexSet<TypeId>) {
        if !out.insert(ty) {
            return;
        }
        let components: Vec<TypeId> = match self.type_ctx.interner.kind(ty) {
            TyKind::Array(element) => vec![*element],
            TyKind::Struct(_, args) | TyKind::Union(_, args) | TyKind::Interface(_, args) => {
                args.clone()
            }
            TyKind::Func(params, ret) => params.iter().copied().chain([*ret]).collect(),
            TyKind::Tuple(elements) => elements.clone(),
            _ => Vec::new(),
        };
        for component in components {
            self.collect_components(component, out);
        }
    }
}
