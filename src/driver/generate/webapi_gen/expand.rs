use super::super::context::GeneratorContext;
use super::analysis::resolve_routes;
use super::attributes::has_attr;
use super::collect::collect_candidates;
use super::collect::collect_functions;
use super::collect::collect_middleware;
use super::collect::has_routes;
use super::emit::emit_extend;
use super::emit::stub_extend;
use super::model::RouteCx;
use crate::driver::source_loader::ProgramAccumulator;
use dream_diagnostics::DiagnosticBag;
use std::collections::HashSet;

pub fn expand_from_acc(
    ctx: &mut GeneratorContext,
    acc: &ProgramAccumulator<'_>,
    diagnostics: &mut DiagnosticBag,
) {
    let wants_pkg = acc.requested_std_packages.contains("system.webapi");
    let has_routes = has_routes(acc);
    if !wants_pkg && !has_routes {
        return;
    }
    if has_routes && !wants_pkg {
        diagnostics.report_error(
            "HTTP route attributes require `import system.webapi;`".to_string(),
            None,
        );
        return;
    }

    let json_names: HashSet<String> = acc
        .all_structs
        .iter()
        .filter(|s| has_attr(&s.attributes, "json"))
        .map(|s| s.name.text.clone())
        .collect();

    let fns = collect_functions(acc);
    let (candidates, collection_errors) = collect_candidates(acc, diagnostics);
    let (routes, binding_errors) = resolve_routes(
        candidates,
        RouteCx {
            fns: &fns,
            json_names: &json_names,
            acc,
        },
        diagnostics,
    );
    if collection_errors || binding_errors {
        ctx.emit_extend("WebApp", stub_extend());
        return;
    }

    let middleware = collect_middleware(acc);
    let source = emit_extend(&routes, &middleware, &json_names, acc);
    ctx.emit_extend("WebApp", source);
}
