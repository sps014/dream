//! Receiver exclusivity is a fixpoint over resolved receiver types and method slots.
//! Explicit qualifiers pin the contract; inferred mutation propagates through sibling
//! calls and registered field types. Source spellings never identify graph vertices.

use super::*;
use crate::function_table::FunctionIdentity;
use dream_syntax::nodes::function::{FunctionNode, ReceiverMode};
use dream_syntax::nodes::statement::StatementNode;
use dream_text::text_span::TextSpan;
use dream_types::TypeId;

type MethodKey = (TypeId, usize);

#[derive(Debug, Clone, PartialEq, Eq)]
enum RecvKind {
    This,
    Field(String),
}

struct Entry {
    owner: TypeId,
    name: String,
    file: Option<Rc<str>>,
    explicit: Option<ReceiverMode>,
    decl_span: TextSpan,
    direct_unique: bool,
    first_mutate_span: Option<TextSpan>,
    raw_calls: Vec<(RecvKind, String, TextSpan)>,
    inferred_unique: bool,
}

impl Entry {
    fn effective_mode(&self) -> ReceiverMode {
        match self.explicit {
            Some(mode) => mode,
            None if self.inferred_unique || self.direct_unique => ReceiverMode::Unique,
            None => ReceiverMode::Borrow,
        }
    }

    fn is_unique(&self) -> bool {
        self.effective_mode() == ReceiverMode::Unique
    }
}

fn new_entry(owner: TypeId, name: &str, method: &FunctionNode, fields: &[String]) -> Entry {
    let mut entry = Entry {
        owner,
        name: name.to_string(),
        file: method.file_path.clone(),
        explicit: method.receiver_mode,
        decl_span: method.name.position,
        direct_unique: false,
        first_mutate_span: None,
        raw_calls: Vec::new(),
        inferred_unique: false,
    };
    walk_body_for_facts(
        method.body,
        fields,
        &mut entry.direct_unique,
        &mut entry.first_mutate_span,
        &mut entry.raw_calls,
    );
    entry
}

impl<'a> Analyzer<'a> {
    // Interface slots retain their dispatch order. Concrete methods follow in stable
    // registration order, including overloads and methods attached by extend blocks.
    fn receiver_function_slots(
        &self,
        owner: TypeId,
    ) -> impl Iterator<Item = (MethodKey, &str, &FunctionIdentity)> {
        let offset = self.interface_methods.get(&owner).map_or(0, Vec::len);
        self.function_table
            .methods
            .iter()
            .filter(move |((ty, _), _)| *ty == owner)
            .flat_map(|((_, name), identities)| {
                identities
                    .iter()
                    .map(move |identity| (name.as_str(), identity))
            })
            .enumerate()
            .map(move |(slot, (name, identity))| ((owner, offset + slot), name, identity))
    }

    pub(super) fn receiver_function_key(
        &self,
        identity: &FunctionIdentity,
    ) -> Option<(MethodKey, String)> {
        self.receiver_slot_entries()
            .find(|(_, _, candidate)| *candidate == identity)
            .map(|(key, name, _)| (key, name.to_string()))
    }

    /// Every concrete method slot, numbered per owner after that owner's interface slots, in one
    /// pass over the method table.
    fn receiver_slot_entries(&self) -> impl Iterator<Item = (MethodKey, &str, &FunctionIdentity)> {
        let mut next: IndexMap<TypeId, usize> = IndexMap::new();
        self.function_table
            .methods
            .iter()
            .flat_map(|((owner, name), identities)| {
                identities
                    .iter()
                    .map(move |identity| (*owner, name.as_str(), identity))
            })
            .map(move |(owner, name, identity)| {
                let slot = next
                    .entry(owner)
                    .or_insert_with(|| self.interface_methods.get(&owner).map_or(0, Vec::len));
                let key = (owner, *slot);
                *slot += 1;
                (key, name, identity)
            })
    }

    pub(super) fn receiver_method_keys(&self, owner: TypeId, name: &str) -> Vec<MethodKey> {
        let mut keys: Vec<_> = self
            .interface_methods
            .get(&owner)
            .into_iter()
            .flatten()
            .enumerate()
            .filter(|(_, method)| method.name.text == name)
            .map(|(slot, _)| (owner, slot))
            .collect();
        keys.extend(
            self.receiver_function_slots(owner)
                .filter(|(_, member, _)| *member == name)
                .map(|(key, _, _)| key),
        );
        keys
    }

    pub(in crate::analyzer) fn classify_receiver_modes(
        &mut self,
        _node: &'a ProgramView<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let scope = self.type_ctx.scope();
        let mut registry: IndexMap<MethodKey, Entry> = IndexMap::new();
        let slots: IndexMap<FunctionIdentity, (MethodKey, String)> = self
            .receiver_slot_entries()
            .map(|(key, name, identity)| (identity.clone(), (key, name.to_string())))
            .collect();
        let methods: Vec<_> = self
            .struct_methods
            .iter()
            .map(|(method, _)| *method)
            .chain(
                self.instantiated_generics
                    .values()
                    .map(|(_, method)| *method),
            )
            .collect();
        for method in methods {
            if method.is_static {
                continue;
            }
            self.type_ctx
                .set_scope(self.graph.module_for_file(method.file_path.as_deref()));
            let Some(identity) = self.function_declaration(method) else {
                continue;
            };
            let Some((key, name)) = slots.get(&identity).cloned() else {
                continue;
            };
            let fields = self
                .struct_table
                .get_struct(key.0)
                .map(|info| info.fields.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            registry.insert(key, new_entry(key.0, &name, method, &fields));
        }
        for (&owner, methods) in &self.interface_methods {
            for (slot, method) in methods.iter().enumerate() {
                registry.insert(
                    (owner, slot),
                    new_entry(owner, &method.name.text, method, &[]),
                );
            }
        }
        self.type_ctx.set_scope(scope);

        let resolved_edges: IndexMap<MethodKey, Vec<(MethodKey, TextSpan)>> = registry
            .iter()
            .map(|(&key, entry)| {
                let mut edges = Vec::new();
                for (recv, name, span) in &entry.raw_calls {
                    let owner = match recv {
                        RecvKind::This => Some(entry.owner),
                        RecvKind::Field(field) => self
                            .struct_table
                            .get_struct(entry.owner)
                            .and_then(|info| info.fields.get(field))
                            .map(|info| info.ty),
                    };
                    if let Some(owner) = owner {
                        edges.extend(
                            self.receiver_method_keys(owner, name)
                                .into_iter()
                                .filter(|target| registry.contains_key(target))
                                .map(|target| (target, *span)),
                        );
                    }
                }
                (key, edges)
            })
            .collect();

        loop {
            let mut changed = false;
            let keys: Vec<_> = registry.keys().copied().collect();
            for key in keys {
                let entry = &registry[&key];
                let should_mark = entry.explicit.is_none()
                    && !entry.inferred_unique
                    && (entry.direct_unique
                        || resolved_edges[&key]
                            .iter()
                            .any(|(target, _)| registry[target].is_unique()));
                if should_mark {
                    registry[&key].inferred_unique = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        self.check_receiver_contracts(diagnostics, &registry, &resolved_edges);
    }
}

mod walking;
use walking::walk_body_for_facts;
mod checking;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::function_table::FunctionTableInfo;
    use dream_types::ModuleId;

    fn add_method(
        analyzer: &mut Analyzer<'_>,
        owner: TypeId,
        argument: TypeId,
    ) -> FunctionIdentity {
        let parameters = vec![owner, argument];
        let def = analyzer
            .type_ctx
            .register_method(owner, "update", &parameters);
        let identity = (def, Vec::new());
        let void = analyzer.type_ctx.interner.void();
        let info =
            FunctionTableInfo::new("update".into(), None, parameters, identity.clone(), void);
        analyzer
            .function_table
            .add_method(owner, "update", info)
            .unwrap();
        identity
    }

    #[test]
    fn same_named_module_receivers_and_overloads_have_distinct_slots() {
        let graph = ModuleGraph::new(Vec::new(), &IndexMap::new());
        let arena = Bump::new();
        let mut analyzer = Analyzer::new(&graph, &arena);
        let mut owners = Vec::new();
        for module in [ModuleId(1), ModuleId(2)] {
            analyzer
                .type_ctx
                .define_module(module, format!("module{}", module.0), Vec::new());
            analyzer.type_ctx.set_scope(module);
            let def = analyzer
                .type_ctx
                .register(DefKind::Struct, "Counter", Vec::new());
            owners.push(analyzer.type_ctx.interner.struct_ty(def, Vec::new()));
        }
        let int = analyzer.type_ctx.interner.int();
        let string = analyzer.type_ctx.interner.string();
        let first = add_method(&mut analyzer, owners[0], int);
        let second = add_method(&mut analyzer, owners[1], int);
        let overload = add_method(&mut analyzer, owners[0], string);
        assert_eq!(
            analyzer.receiver_function_key(&first).unwrap().0,
            (owners[0], 0)
        );
        assert_eq!(
            analyzer.receiver_function_key(&second).unwrap().0,
            (owners[1], 0)
        );
        assert_eq!(
            analyzer.receiver_function_key(&overload).unwrap().0,
            (owners[0], 1)
        );
        assert_eq!(
            analyzer.receiver_method_keys(owners[0], "update"),
            vec![(owners[0], 0), (owners[0], 1)]
        );
        analyzer
            .receiver_modes
            .insert((owners[0], 0), ReceiverMode::Unique);
        analyzer
            .receiver_modes
            .insert((owners[1], 0), ReceiverMode::Borrow);
        assert_eq!(
            analyzer.receiver_modes[&(owners[0], 0)],
            ReceiverMode::Unique
        );
        assert_eq!(
            analyzer.receiver_modes[&(owners[1], 0)],
            ReceiverMode::Borrow
        );
    }

    #[test]
    fn interface_extensions_follow_dispatch_slots() {
        let graph = ModuleGraph::new(Vec::new(), &IndexMap::new());
        let arena = Bump::new();
        let mut analyzer = Analyzer::new(&graph, &arena);
        let def = analyzer
            .type_ctx
            .register(DefKind::Interface, "Counter", Vec::new());
        let owner = analyzer.type_ctx.interner.interface_ty(def, Vec::new());
        let method = arena.alloc(FunctionNode::new(
            Vec::new(),
            synthetic_token(TokenKind::IdentifierToken, "update"),
            None,
            None,
            Vec::new(),
            &[],
            dream_syntax::nodes::Visibility::Public,
        ));
        analyzer.interface_methods.insert(owner, vec![method]);
        let int = analyzer.type_ctx.interner.int();
        let extension = add_method(&mut analyzer, owner, int);
        assert_eq!(
            analyzer.receiver_function_key(&extension).unwrap().0,
            (owner, 1)
        );
        assert_eq!(
            analyzer.receiver_method_keys(owner, "update"),
            vec![(owner, 0), (owner, 1)]
        );
    }
}
