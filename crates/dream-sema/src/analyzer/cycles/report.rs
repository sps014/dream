//! Turns unresolvable stores into diagnostics that spell out the reference cycle.

use super::super::*;
use super::summary::{Finding, Origin};
use super::type_graph::{Edge, TypeGraph};
use super::value::Slot;
use dream_syntax::nodes::StructDeclarationNode;
use dream_text::text_span::TextSpan;
use dream_types::{DefId, TyKind, TypeId};
use indexmap::IndexSet;
use std::collections::VecDeque;

/// Further stores closing the same cycle are listed up to this many notes.
const MAX_STORES: usize = 3;

type Decls<'n, 'a> = IndexMap<DefId, &'n StructDeclarationNode<'a>>;

/// One reported cycle, merged across every store that can close it.
struct Report {
    chain: String,
    span: Option<TextSpan>,
    file: Option<String>,
    /// `Owner.field` to make `weak`/`unowned`, when the cycle crosses a declared field.
    field: Option<String>,
    /// The field's type when it is a standard collection, which owns its elements.
    collection: Option<String>,
    /// The cycle crosses another declared field that could become the weak link instead.
    other_links: bool,
    class: Option<String>,
    stores: Vec<String>,
    more: usize,
}

impl<'a> Analyzer<'a> {
    pub(super) fn report_cycles(
        &self,
        node: &'a ProgramView<'a>,
        graph: &TypeGraph<'_>,
        findings: Vec<Finding>,
        diagnostics: &mut DiagnosticBag,
    ) {
        if findings.is_empty() {
            return;
        }
        let mut decls: Decls<'_, 'a> = IndexMap::new();
        for declaration in &node.structs {
            let scope = self.graph.module_for_file(declaration.file_path.as_deref());
            if let Some(def) =
                self.type_ctx
                    .resolve_from(scope, DefKind::Struct, &declaration.name.text)
            {
                decls.insert(def, declaration);
            }
        }
        let interner = &self.type_ctx.interner;
        let mut reports: IndexMap<(Option<(TypeId, u32)>, String), Report> = IndexMap::new();
        for finding in findings {
            let cycle = cycle(graph, finding.origin);
            let classes: IndexSet<DefId> = cycle
                .iter()
                .filter_map(|&(ty, _)| match interner.kind(ty) {
                    TyKind::Struct(def, _) if !interner.is_value_type(ty) => Some(*def),
                    _ => None,
                })
                .collect();
            let allowed = classes.iter().any(|def| {
                decls
                    .get(def)
                    .is_some_and(|d| dream_abi::attributes::allows_cycle(&d.attributes))
            });
            if allowed {
                continue;
            }
            let store = match &finding.file {
                Some(file) => format!("{file}:{}", finding.line),
                None => format!("line {}", finding.line),
            };
            // Suggest a field the user declared, a direct link over a collection (a weak
            // collection would be freed at once), a class reference over an inline value-struct
            // field, and then the link the closing store goes through: the stored slot itself,
            // or the last field leading back to it.
            let fields: Vec<_> = cycle
                .iter()
                .enumerate()
                .filter_map(|(at, &(_, edge))| match edge {
                    Edge::Field(owner, index) => match interner.kind(owner) {
                        TyKind::Struct(def, _) => decls.get(def).map(|d| (at, owner, index, *d)),
                        _ => None,
                    },
                    _ => None,
                })
                .collect();
            let fix = fields
                .iter()
                .copied()
                .min_by_key(|&(at, owner, index, d)| {
                    let distance = (cycle.len() - at) % cycle.len();
                    (
                        is_std(d),
                        self.collection(graph, owner, index).is_some(),
                        interner.is_value_type(owner),
                        distance,
                    )
                });
            let start = fix.map_or(0, |(at, ..)| at);
            let chain = self.cycle_chain(graph, &cycle, start);
            let key = match fix {
                Some((_, owner, index, _)) => (Some((owner, index)), String::new()),
                None => (None, chain.clone()),
            };
            if let Some(report) = reports.get_mut(&key) {
                if report.stores.contains(&store) {
                    continue;
                }
                if report.stores.len() < MAX_STORES {
                    report.stores.push(store);
                } else {
                    report.more += 1;
                }
                continue;
            }
            let mut report = Report {
                chain,
                span: None,
                file: finding.file.clone(),
                field: None,
                collection: None,
                other_links: fields.iter().filter(|&&(.., d)| !is_std(d)).count() > 1,
                class: None,
                stores: vec![store],
                more: 0,
            };
            if let Some((_, owner, index, declaration)) = fix {
                let name = field_name(graph, owner, index);
                report.span = Some(
                    declaration
                        .fields
                        .iter()
                        .find(|f| f.name.text == name)
                        .map_or(declaration.name.position, |f| f.name.position),
                );
                report.file = file_path_string(&declaration.file_path);
                report.field = Some(format!("{}.{name}", declaration.name.text));
                report.collection = self
                    .collection(graph, owner, index)
                    .map(|ty| self.type_id_display(ty));
                report.class = if interner.is_value_type(owner) {
                    let user = |def: &&DefId| decls.get(*def).is_some_and(|d| !is_std(d));
                    classes
                        .iter()
                        .find(user)
                        .or(classes.first())
                        .map(|def| self.type_ctx.defs.name(*def).to_string())
                } else {
                    Some(declaration.name.text.clone())
                };
            }
            reports.insert(key, report);
        }
        for report in reports.into_values() {
            diagnostics.report(report.into_diagnostic());
        }
    }

    /// Renders the cycle as `A.field -> B -> A`, starting at `start`. Wrappers that add no
    /// ownership of their own (`Option`, tuples, erased `object`, a std collection's backing
    /// array) are left out so the chain reads in terms of the user's types.
    fn cycle_chain(&self, graph: &TypeGraph<'_>, cycle: &[(TypeId, Edge)], start: usize) -> String {
        let interner = &self.type_ctx.interner;
        let user = |ty: TypeId| match interner.kind(ty) {
            TyKind::Struct(def, _) | TyKind::Union(def, _) => Some(!self.std_def(*def)),
            _ => None,
        };
        let mut parts: Vec<String> = Vec::new();
        let mut after_std = false;
        for k in 0..cycle.len() {
            let (owner, edge) = cycle[(start + k) % cycle.len()];
            let kind = interner.kind(owner);
            let declared = user(owner);
            let hidden = k > 0
                && (matches!(kind, TyKind::Object)
                    || edge == Edge::Inline
                    || (matches!(edge, Edge::Payload(..)) && declared == Some(false))
                    || (matches!(kind, TyKind::Array(_)) && after_std));
            after_std = declared == Some(false);
            if hidden {
                continue;
            }
            let name = self.type_id_display(owner);
            parts.push(match edge {
                Edge::Field(_, index) if declared == Some(true) || k == 0 => {
                    format!("{name}.{}", field_name(graph, owner, index))
                }
                Edge::Payload(_, variant, field) => {
                    let variant = graph
                        .layouts()
                        .unions
                        .get(&owner)
                        .and_then(|layout| layout.variants.get(variant as usize));
                    match variant {
                        Some(v) => {
                            let field = v.fields.get(field as usize).map_or("?", |f| &f.name);
                            format!("{name}.{}.{field}", v.name)
                        }
                        None => name,
                    }
                }
                Edge::Erased if matches!(kind, TyKind::Func(..)) => format!("closure {name}"),
                _ => name,
            });
        }
        if let Some(&(first, _)) = cycle.get(start) {
            parts.push(self.type_id_display(first));
        }
        parts.join(" -> ")
    }

    fn std_def(&self, def: DefId) -> bool {
        let module = &self.type_ctx.defs.get(def).module_path;
        module == "system" || module.starts_with("system.")
    }

    /// The type of field `index` of `owner` when it is a standard collection class.
    fn collection(&self, graph: &TypeGraph<'_>, owner: TypeId, index: u32) -> Option<TypeId> {
        let ty = graph.layouts().structs.get(&owner)?.fields.get(index as usize)?.ty;
        let interner = &self.type_ctx.interner;
        match interner.kind(ty) {
            TyKind::Struct(def, _) if !interner.is_value_type(ty) && self.std_def(*def) => Some(ty),
            _ => None,
        }
    }
}

impl Report {
    fn into_diagnostic(self) -> Diagnostic {
        let mut diagnostic = Diagnostic::new(
            format!("reference cycle: {}", self.chain),
            self.span,
            self.file,
        )
        .with_note("objects in this cycle keep each other alive, so none of them is ever freed");
        for store in &self.stores {
            diagnostic = diagnostic.with_note(format!("the cycle is closed by the store at {store}"));
        }
        if self.more > 0 {
            let plural = if self.more == 1 { "" } else { "s" };
            diagnostic = diagnostic.with_note(format!("... and {} more store{plural}", self.more));
        }
        diagnostic = match (&self.field, &self.collection) {
            (Some(field), Some(collection)) => {
                let alternative = if self.other_links {
                    ", or make another link in the cycle weak or unowned"
                } else {
                    ""
                };
                diagnostic.with_help(format!(
                    "the {collection} in '{field}' keeps its elements alive; do not add anything to it that leads back to its owner{alternative}"
                ))
            }
            (Some(field), None) => diagnostic.with_help(format!(
                "make '{field}' weak or unowned so it does not keep its target alive"
            )),
            (None, _) => diagnostic.with_help("make one link in this cycle weak or unowned"),
        };
        if let Some(class) = &self.class {
            diagnostic = diagnostic.with_help(format!(
                "if the program breaks this cycle itself, add @allow_cycle to class '{class}'"
            ));
        }
        diagnostic
    }
}

fn is_std(declaration: &StructDeclarationNode<'_>) -> bool {
    declaration
        .file_path
        .as_deref()
        .is_some_and(dream_stdlib::is_std_source)
}

fn field_name<'g>(graph: &'g TypeGraph<'_>, owner: TypeId, index: u32) -> &'g str {
    graph
        .layouts()
        .structs
        .get(&owner)
        .and_then(|layout| layout.fields.get(index as usize))
        .map_or("?", |f| f.name.as_str())
}

/// The strong edges of one cycle through `origin`: the stored slot, then a shortest path from
/// the stored value back to the container. Each entry is `(owner type, edge)`.
fn cycle(graph: &TypeGraph<'_>, origin: Origin) -> Vec<(TypeId, Edge)> {
    let first = match origin.slot {
        Slot::Field(owner, index) | Slot::Weak(owner, index) => Edge::Field(owner, index),
        Slot::Payload(owner, variant, field) => Edge::Payload(owner, variant, field),
        Slot::Elem(_) => Edge::Elem,
    };
    let mut out = vec![(origin.container, first)];
    if origin.value == origin.container {
        return out;
    }
    let mut parent: IndexMap<TypeId, Option<(TypeId, Edge)>> = IndexMap::new();
    parent.insert(origin.value, None);
    let mut queue = VecDeque::from([origin.value]);
    while let Some(ty) = queue.pop_front() {
        if ty == origin.container {
            break;
        }
        for (child, edge) in graph.children(ty) {
            if !parent.contains_key(&child) && graph.same_component(child, origin.container) {
                parent.insert(child, Some((ty, edge)));
                queue.push_back(child);
            }
        }
    }
    let mut path = Vec::new();
    let mut at = origin.container;
    while let Some(Some((from, edge))) = parent.get(&at) {
        path.push((*from, *edge));
        at = *from;
    }
    path.reverse();
    out.extend(path);
    out
}
