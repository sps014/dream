//! Compile-time detection of ARC reference cycles between `class` types, plus validation of the
//! `weak`/`unowned` field modifiers that break them.
//!
//! [`Analyzer::check_reference_cycles`] builds a directed graph whose nodes are non-generic
//! `class` declarations and whose edges are strong (non-`weak`/`unowned`) fields that hold (or,
//! through `Option<T>`/`T[]`/`List`/`Map`/`Set`, transitively hold) a reference to another class.
//! Any strongly connected component of that graph — including a self-loop — is a leak the
//! runtime's ARC can never collect, and is reported as a hard compile error unless every class in
//! the cycle carries `@allow_cycle`. See `docs/language/memory.md` and the design note referenced
//! there for the full rationale, including why this is a *structural* check (it cannot see cycles
//! assembled dynamically through `object`/callbacks).

use super::*;
use dream_syntax::nodes::struct_node::StructDeclarationNode;
use dream_types::{DefKind, TyKind, TypeId};
use indexmap::{IndexMap, IndexSet};

/// One strong-reference edge in the class reference-cycle graph: field `field_name` (declared at
/// `field_position`) of the owning class holds a class-typed value of `to`.
struct ClassEdge {
    field_name: String,
    field_position: Option<TextSpan>,
    to: TypeId,
}

impl<'a> Analyzer<'a> {
    /// Validates every `weak`/`unowned` field's shape, then runs the whole-program class
    /// reference-cycle check. Called from [`Self::register_structs`] once every non-generic class
    /// is registered in `self.struct_table` (so field types can be classified as value/reference).
    pub(in crate::analyzer) fn check_weak_unowned_and_cycles(
        &mut self,
        node: &'a ProgramView<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        self.validate_weak_unowned_fields(node, diagnostics);
        self.check_reference_cycles(node, diagnostics);
    }

    /// The registered type of a non-generic struct/class declaration, resolved in its own module.
    fn declared_struct_type(&self, decl: &StructDeclarationNode<'a>) -> Option<TypeId> {
        let module = self.graph.module_for_file(decl.file_path.as_deref());
        let def = self
            .type_ctx
            .declared_in(module, DefKind::Struct, &decl.name.text)?;
        self.type_ctx
            .interner
            .lookup(&TyKind::Struct(def, Vec::new()))
    }

    fn is_class_type(&self, ty: TypeId) -> bool {
        self.struct_info(ty).is_some_and(|info| !info.is_value)
    }

    /// `weak` fields must be `Option<T>` for a class `T`; `unowned` fields must themselves be a
    /// bare class type `T`; a field cannot be both.
    fn validate_weak_unowned_fields(
        &mut self,
        node: &'a ProgramView<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        for struct_decl in node.structs.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(struct_decl.file_path.as_deref()));
            for field in &struct_decl.fields {
                if !field.is_weak && !field.is_unowned {
                    continue;
                }
                if field.is_weak && field.is_unowned {
                    diagnostics.report_error(
                        format!(
                            "field '{}' cannot be both 'weak' and 'unowned'",
                            field.name.text
                        ),
                        Some(field.name.position),
                    );
                    continue;
                }
                let ty = self.type_ctx.lower(&field.field_type);
                if field.is_weak {
                    let is_class_option = match self.type_ctx.interner.kind(ty) {
                        TyKind::Union(def, args) if args.len() == 1 => {
                            self.type_ctx.defs.name(*def) == "Option"
                                && self.is_class_type(args[0])
                        }
                        _ => false,
                    };
                    if !is_class_option {
                        diagnostics.report_error(
                            format!(
                                "'weak' field '{}' must have type 'Option<T>' where 'T' is a class, got '{}'",
                                field.name.text,
                                field.field_type.display_name()
                            ),
                            Some(field.name.position),
                        );
                    }
                } else if !self.is_class_type(ty) {
                    diagnostics.report_error(
                        format!(
                            "'unowned' field '{}' must have a class type, got '{}'",
                            field.name.text,
                            field.field_type.display_name()
                        ),
                        Some(field.name.position),
                    );
                }
            }
        }
    }

    /// Builds the strong-reference graph over non-generic `class` declarations and hard-errors on
    /// every strongly connected component (Tarjan's SCC), unless every class in it carries
    /// `@allow_cycle`.
    fn check_reference_cycles(&self, node: &'a ProgramView<'a>, diagnostics: &mut DiagnosticBag) {
        // Value structs holding references participate as edges (see `strong_ref_targets`).
        let ref_values = self.ref_holding_value_structs();

        let mut edges: IndexMap<TypeId, Vec<ClassEdge>> = IndexMap::new();
        let mut allow_cycle: IndexSet<TypeId> = IndexSet::new();
        let mut names: IndexMap<TypeId, (String, TextSpan)> = IndexMap::new();

        for struct_decl in node.structs.iter() {
            // Generic templates aren't monomorphized here (their field types aren't concrete
            // yet); value structs already have their own containment check (they can't hold a
            // strong cycle at all, since embedding a class only ever takes a reference).
            if struct_decl.generic_parameters.is_some() || struct_decl.is_value {
                continue;
            }
            let Some(ty) = self.declared_struct_type(struct_decl) else {
                continue;
            };
            let Some(info) = self.struct_info(ty) else {
                continue;
            };
            names.insert(
                ty,
                (struct_decl.name.text.clone(), struct_decl.name.position),
            );
            if struct_decl
                .attributes
                .iter()
                .any(|a| a.name.text == "allow_cycle")
            {
                allow_cycle.insert(ty);
            }

            let mut out = Vec::new();
            for field in &struct_decl.fields {
                if field.is_weak || field.is_unowned {
                    continue;
                }
                let Some(field_info) = info.fields.get(&field.name.text) else {
                    continue;
                };
                for target in self.strong_ref_targets(field_info.ty, &ref_values, &mut IndexSet::new()) {
                    out.push(ClassEdge {
                        field_name: field.name.text.clone(),
                        field_position: Some(field.name.position),
                        to: target,
                    });
                }
            }
            edges.entry(ty).or_default().extend(out);
        }

        let nodes: Vec<TypeId> = edges.keys().copied().collect();
        let sccs = tarjan_scc(&nodes, &edges);

        for scc in sccs {
            let self_loop = scc.len() == 1
                && edges
                    .get(&scc[0])
                    .map(|es| es.iter().any(|e| e.to == scc[0]))
                    .unwrap_or(false);
            if scc.len() < 2 && !self_loop {
                continue;
            }
            // `@allow_cycle` only suppresses a cycle when *every* class participating in it opted
            // in; a cycle merely passing through one annotated class among several is still an
            // error, so the escape hatch can't be laundered onto a multi-class cycle.
            if scc.iter().all(|c| allow_cycle.contains(c)) {
                continue;
            }

            let scc_set: IndexSet<TypeId> = scc.iter().copied().collect();
            let mut culprits: Vec<(String, Option<TextSpan>)> = Vec::new();
            for class in &scc {
                let Some((class_name, class_pos)) = names.get(class) else {
                    continue;
                };
                if let Some(es) = edges.get(class) {
                    for e in es {
                        if scc_set.contains(&e.to) {
                            culprits.push((
                                format!("'{}.{}'", class_name, e.field_name),
                                e.field_position.or(Some(*class_pos)),
                            ));
                        }
                    }
                }
            }
            culprits.sort_by(|a, b| a.0.cmp(&b.0));
            culprits.dedup_by(|a, b| a.0 == b.0);
            let position = culprits.first().and_then(|c| c.1);
            let list = culprits
                .iter()
                .map(|c| c.0.as_str())
                .collect::<Vec<_>>()
                .join(", ");

            diagnostics.report_error(
                format!(
                    "reference cycle detected: {} form a strong-reference cycle, so none of their objects can ever be freed; mark one field 'weak' or 'unowned' to break it, or annotate every class in the cycle with '@allow_cycle' if the cycle is intentional",
                    list
                ),
                position,
            );
        }
    }

    /// The classes transitively strong-referenced by `ty`: `ty` itself if it is a class, or
    /// (recursively) the element type of `T[]` / tuples / the type arguments of `Option`, `List`,
    /// `Set` and `Map`. A **value struct** whose fields transitively hold references counts too —
    /// its inline storage keeps those elements alive, so `class C { h: Holder }` +
    /// `struct Holder { d: Data }` + `Data -> C` is a detected cycle. Interface-typed values resolve
    /// to every implementing class (a conservative over-approximation that catches cross-interface
    /// cycles). Primitives, reference-free value structs, `object`, and callbacks contribute no
    /// edge. `visited` guards re-entering value structs that reference each other.
    fn strong_ref_targets(
        &self,
        ty: TypeId,
        ref_values: &IndexSet<TypeId>,
        visited: &mut IndexSet<TypeId>,
    ) -> Vec<TypeId> {
        match self.type_ctx.interner.kind(ty) {
            TyKind::Array(inner) => self.strong_ref_targets(*inner, ref_values, visited),
            TyKind::Tuple(elements) => elements
                .iter()
                .flat_map(|&e| self.strong_ref_targets(e, ref_values, visited))
                .collect(),
            TyKind::Struct(def, args) | TyKind::Union(def, args)
                if matches!(self.type_ctx.defs.name(*def), "Option" | "List" | "Set" | "Map") =>
            {
                args.iter()
                    .flat_map(|&a| self.strong_ref_targets(a, ref_values, visited))
                    .collect()
            }
            TyKind::Struct(..) => match self.struct_info(ty) {
                Some(info) if !info.is_value => vec![ty],
                Some(info) if ref_values.contains(&ty) && visited.insert(ty) => info
                    .fields
                    .values()
                    .flat_map(|f| self.strong_ref_targets(f.ty, ref_values, visited))
                    .collect(),
                _ => Vec::new(),
            },
            TyKind::Interface(..) => self
                .implements
                .iter()
                .filter(|(_, ifaces)| ifaces.contains(&ty))
                .map(|(class, _)| *class)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Value structs whose fields transitively hold an RC-tracked value. Computed to a fixed
    /// point so nested value structs (`struct Outer { inner: Cell }`) resolve without looping
    /// on declaration cycles among value structs themselves.
    fn ref_holding_value_structs(&self) -> IndexSet<TypeId> {
        let value_types: Vec<TypeId> = self
            .struct_table
            .structs
            .iter()
            .filter(|(_, info)| info.is_value)
            .map(|(&ty, _)| ty)
            .collect();
        let mut out = IndexSet::new();
        loop {
            let mut grew = false;
            for &ty in &value_types {
                if out.contains(&ty) {
                    continue;
                }
                let Some(info) = self.struct_info(ty) else {
                    continue;
                };
                if info.fields.values().any(|f| self.holds_managed(f.ty, &out)) {
                    out.insert(ty);
                    grew = true;
                }
            }
            if !grew {
                return out;
            }
        }
    }

    fn holds_managed(&self, ty: TypeId, ref_values: &IndexSet<TypeId>) -> bool {
        match self.type_ctx.interner.kind(ty) {
            TyKind::Prim(dream_types::PrimTy::String) | TyKind::Object => true,
            TyKind::Array(_) => true,
            TyKind::Tuple(elements) => elements.iter().any(|&e| self.holds_managed(e, ref_values)),
            TyKind::Struct(..) => match self.struct_info(ty) {
                // Another value struct: resolved by a later fixed-point pass.
                Some(info) if info.is_value => ref_values.contains(&ty),
                Some(_) => true,
                None => false,
            },
            _ => !self
                .strong_ref_targets(ty, ref_values, &mut IndexSet::new())
                .is_empty(),
        }
    }
}

/// Tarjan's strongly-connected-components algorithm over the class strong-reference graph.
/// Returns every SCC (including singletons with no self-loop, which callers filter out).
fn tarjan_scc(nodes: &[TypeId], edges: &IndexMap<TypeId, Vec<ClassEdge>>) -> Vec<Vec<TypeId>> {
    #[derive(Default)]
    struct State {
        index_counter: usize,
        stack: Vec<TypeId>,
        indices: IndexMap<TypeId, usize>,
        lowlink: IndexMap<TypeId, usize>,
        on_stack: IndexSet<TypeId>,
        result: Vec<Vec<TypeId>>,
    }

    fn strongconnect(v: TypeId, edges: &IndexMap<TypeId, Vec<ClassEdge>>, st: &mut State) {
        let idx = st.index_counter;
        st.indices.insert(v, idx);
        st.lowlink.insert(v, idx);
        st.index_counter += 1;
        st.stack.push(v);
        st.on_stack.insert(v);

        for e in edges.get(&v).into_iter().flatten() {
            let w = e.to;
            let candidate = if !st.indices.contains_key(&w) {
                strongconnect(w, edges, st);
                st.lowlink[&w]
            } else if st.on_stack.contains(&w) {
                st.indices[&w]
            } else {
                continue;
            };
            let Some(v_low) = st.lowlink.get_mut(&v) else {
                crate::internal_error!("Tarjan node {v:?} lost its lowlink");
            };
            *v_low = (*v_low).min(candidate);
        }

        if st.lowlink[&v] == st.indices[&v] {
            let mut component = Vec::new();
            loop {
                let Some(w) = st.stack.pop() else {
                    crate::internal_error!("Tarjan stack emptied before node {v:?}");
                };
                st.on_stack.swap_remove(&w);
                component.push(w);
                if w == v {
                    break;
                }
            }
            st.result.push(component);
        }
    }

    let mut st = State::default();
    for &n in nodes {
        if !st.indices.contains_key(&n) {
            strongconnect(n, edges, &mut st);
        }
    }
    st.result
}
