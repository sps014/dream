use super::collection_expr::collect_collections_from_expr;
use super::collection_stmts::collect_collections_from_stmts;
use super::collection_types::CollectionSpec;
use crate::driver::source_loader::ProgramAccumulator;
use dream_syntax::nodes::FunctionNode;
use std::collections::BTreeSet;
use std::collections::HashSet;

pub(super) fn is_user_source(path: Option<&std::rc::Rc<str>>) -> bool {
    match path {
        Some(p) => !dream_stdlib::is_std_source(p),
        None => true,
    }
}

/// Top-level collection adapters for `Json.serialize` / `deserialize` / `from_value` /
/// `serialize_pretty`. `@json` field collections are inlined in generated `to_json`.
pub(super) fn collect_from_function(
    f: &FunctionNode<'_>,
    jsonable: &HashSet<String>,
    out: &mut BTreeSet<CollectionSpec>,
) {
    let mut locals = Vec::new();
    for p in &f.parameters {
        if p.name.text != "this" {
            locals.push((p.name.text.clone(), p.type_.clone()));
        }
    }
    for stmt in f.body {
        collect_collections_from_stmts(stmt, jsonable, out, &mut locals);
    }
}

pub(super) fn collect_all_collections(
    acc: &ProgramAccumulator<'_>,
    jsonable: &HashSet<String>,
) -> Vec<CollectionSpec> {
    let mut out = BTreeSet::new();
    for g in &acc.all_globals {
        if !is_user_source(g.file_path.as_ref()) {
            continue;
        }
        collect_collections_from_expr(&g.initializer, jsonable, &mut out, &[]);
    }
    for f in &acc.all_functions {
        if !is_user_source(f.file_path.as_ref()) {
            continue;
        }
        collect_from_function(f, jsonable, &mut out);
    }
    for s in &acc.all_structs {
        if !is_user_source(s.file_path.as_ref()) {
            continue;
        }
        for m in &s.methods {
            collect_from_function(m, jsonable, &mut out);
        }
    }
    for e in &acc.all_extends {
        if e.is_synthesized || !is_user_source(e.file_path.as_ref()) {
            continue;
        }
        for m in &e.methods {
            collect_from_function(m, jsonable, &mut out);
        }
    }
    out.into_iter().collect()
}
