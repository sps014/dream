use super::model::*;
use super::registry::RegisteredGenerator;
use super::snapshot::{Builder, BuiltSnapshot, ProgramFacts, build_selected};
use crate::driver::source_loader::ProgramAccumulator;

/// Triggered declarations and their bounded transitive dependencies share one identity/span map.
pub fn build_snapshot(
    acc: &ProgramAccumulator<'_>,
    facts: &ProgramFacts<'_>,
    registered: &RegisteredGenerator,
) -> Result<BuiltSnapshot, String> {
    let mut built = build_selected(acc, facts, registered, false)?;
    let catalog_built = build_selected(acc, facts, registered, true)?;
    let catalog = catalog_built.snapshot;
    built.snapshot.symbols = catalog
        .decls
        .iter()
        .map(|d| SymbolView {
            id: d.id.clone(),
            kind: d.kind.clone(),
            declaration: Some(d.clone()),
            instances: Vec::new(),
            function: None,
        })
        .chain(catalog.functions.iter().map(|f| SymbolView {
            id: if f.overload.is_empty() {
                f.id.clone()
            } else {
                f.overload.clone()
            },
            kind: "function".to_string(),
            declaration: None,
            instances: Vec::new(),
            function: Some(f.clone()),
        }))
        .collect();
    super::symbol_instances::attach(&mut built.snapshot.symbols, facts);
    let mut b = Builder {
        facts,
        spans: built.spans,
        include_bodies: true,
        include_std_bodies: registered.is_std(),
    };
    b.spans.extend_definitions(catalog_built.spans);
    super::references::expand(&mut built.snapshot, |id| {
        if let Some(f) = catalog
            .functions
            .iter()
            .find(|f| f.id == id || f.overload == id)
        {
            let source = acc.all_functions.iter().find(|source| {
                source.name.text == f.name
                    && source.name.position.line_no as u32 == f.location.line
                    && source.name.position.col_no as u32 == f.location.column
                    && super::decls::module_of(acc, source.file_path.as_deref())
                        == f.id.rsplit_once("::").map(|(m, _)| m).unwrap_or_default()
            })?;
            return Some(super::references::Reference::Function(Box::new(
                b.function(
                    None,
                    &super::decls::module_of(acc, source.file_path.as_deref()),
                    source,
                    source.file_path.as_deref(),
                    &[],
                ),
            )));
        }
        let d = catalog
            .decls
            .iter()
            .find(|d| d.id == id || id.starts_with(&format!("{}.", d.id)))?;
        let mut d = d.clone();
        if let Some(source) = acc.all_structs.iter().find(|s| {
            s.name.text == d.name
                && super::decls::module_of(acc, s.file_path.as_deref()) == d.module
        }) {
            b.mark(&d.id, source.file_path.as_deref(), source.name.position);
            d.methods = source
                .methods
                .iter()
                .map(|m| {
                    b.function(
                        Some(&d.id),
                        &d.module,
                        m,
                        source.file_path.as_deref(),
                        &d.generics,
                    )
                })
                .collect();
        } else if let Some(source) = acc.all_enums.iter().find(|s| {
            s.name.text == d.name
                && super::decls::module_of(acc, s.file_path.as_deref()) == d.module
        }) {
            b.mark(&d.id, source.file_path.as_deref(), source.name.position);
            d.methods = source
                .methods
                .iter()
                .map(|m| {
                    b.function(
                        Some(&d.id),
                        &d.module,
                        m,
                        source.file_path.as_deref(),
                        &d.generics,
                    )
                })
                .collect();
        }
        Some(super::references::Reference::Decl(Box::new(d)))
    })?;
    let selected: Vec<_> = built
        .snapshot
        .decls
        .iter()
        .map(|d| d.id.as_str())
        .chain(built.snapshot.functions.iter().map(|f| f.id.as_str()))
        .collect();
    for diagnostic in &facts.semantic.diagnostics {
        if diagnostic.severity != dream_diagnostics::Severity::Error {
            continue;
        }
        let Some(span) = diagnostic.span else {
            continue;
        };
        let Some(target) = b.spans.covering(diagnostic.file_path.as_deref(), span) else {
            continue;
        };
        if selected.iter().any(|id| {
            target == *id
                || target.starts_with(&format!("{id}."))
                || target.starts_with(&format!("{id}#"))
        }) {
            built.snapshot.compilation_errors.push(GenDiagnostic {
                severity: "error".into(),
                message: diagnostic.message.clone(),
                target: target.to_string(),
                ..Default::default()
            });
        }
    }
    b.spans
        .consumable
        .retain(|target, _| catalog.functions.iter().filter(|f| &f.id == target).count() == 1);
    built.spans = b.spans;
    // The catalog behind `symbols` lists every declaration, and semantic type views list every
    // interned instance. Keep only the trigger closure: an unrelated instantiation must not change
    // the snapshot the generator reads.
    trim_to_closure(&mut built.snapshot);
    Ok(built)
}

/// Drops symbol views and instances the generator's trigger closure does not name.
fn trim_to_closure(snapshot: &mut Snapshot) {
    use indexmap::IndexSet;
    let mut ids = IndexSet::new();
    for decl in snapshot.decls.iter().chain(&snapshot.referenced) {
        ids.insert(decl.id.clone());
    }
    for function in snapshot
        .functions
        .iter()
        .chain(&snapshot.referenced_functions)
    {
        ids.insert(function.id.clone());
        if !function.overload.is_empty() {
            ids.insert(function.overload.clone());
        }
    }
    snapshot.symbols.retain(|symbol| {
        ids.contains(&symbol.id)
            || symbol.function.as_ref().is_some_and(|function| {
                ids.contains(&function.id)
                    || (!function.overload.is_empty() && ids.contains(&function.overload))
            })
    });
    let mut types = Vec::new();
    for decl in snapshot.decls.iter().chain(&snapshot.referenced) {
        collect_decl_types(decl, &mut types);
    }
    for function in snapshot
        .functions
        .iter()
        .chain(&snapshot.referenced_functions)
    {
        collect_function_types(function, &mut types);
    }
    for call in &snapshot.calls {
        types.extend(call.type_args.iter().cloned());
        types.extend(call.arg_types.iter().cloned());
    }
    for symbol in &mut snapshot.symbols {
        symbol
            .instances
            .retain(|instance| types.iter().any(|ty| ty == &instance.ty));
    }
}

fn collect_decl_types(decl: &Decl, out: &mut Vec<TypeRef>) {
    out.extend(decl.implements.iter().cloned());
    for field in &decl.fields {
        out.push(field.ty.clone());
    }
    for variant in &decl.variants {
        for field in &variant.fields {
            out.push(field.ty.clone());
        }
    }
    for method in &decl.methods {
        collect_function_types(method, out);
    }
}

fn collect_function_types(function: &Function, out: &mut Vec<TypeRef>) {
    for param in &function.params {
        out.push(param.ty.clone());
    }
    out.push(function.ret.clone());
    if let Some(body) = &function.body {
        for statement in &body.statements {
            collect_node_types(statement, out);
        }
    }
}

fn collect_node_types(node: &BodyNode, out: &mut Vec<TypeRef>) {
    out.extend(node.types.iter().cloned());
    out.push(node.ty.clone());
    for child in &node.children {
        collect_node_types(child, out);
    }
}
