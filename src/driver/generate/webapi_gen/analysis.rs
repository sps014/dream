use super::attributes::attr_enum_name;
use super::attributes::attr_enum_names;
use super::attributes::attr_string;
use super::attributes::has_attr;
use super::attributes::join_http_path;
use super::model::ParamKind;
use super::model::ParamPlan;
use super::model::Route;
use super::model::RouteCandidate;
use super::model::RouteCx;
use super::model::RouteKind;
use super::model::RouteSpec;
use super::types::option_inner;
use super::types::path_placeholders;
use super::types::type_name;
use crate::driver::source_loader::ProgramAccumulator;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::function::FunctionNode;
use indexmap::IndexMap;
use std::collections::HashSet;

pub(super) fn is_middleware_shape(f: &FunctionNode<'_>) -> bool {
    if !f.is_async {
        return false;
    }
    let ret = f
        .return_type
        .as_ref()
        .map(type_name)
        .unwrap_or_else(|| "void".into());
    if ret != "HttpOutgoing" {
        return false;
    }
    if f.parameters.len() != 2 {
        return false;
    }
    type_name(&f.parameters[0].type_) == "RequestContext"
        && type_name(&f.parameters[1].type_) == "Next"
}

pub(super) fn validate_uses(
    uses: &[String],
    route: &FunctionNode<'_>,
    fns: &IndexMap<String, &FunctionNode<'_>>,
    diagnostics: &mut DiagnosticBag,
) -> bool {
    let mut ok = true;
    for name in uses {
        let Some(target) = fns.get(name).copied() else {
            report(
                diagnostics,
                route,
                format!("@use({name}) does not resolve to a function"),
            );
            ok = false;
            continue;
        };
        if !is_middleware_shape(target) {
            report(
                diagnostics,
                route,
                format!(
                    "@use({name}) must be `async fun {name}(ctx: RequestContext, next: Next): HttpOutgoing`"
                ),
            );
            ok = false;
        }
    }
    ok
}

pub(super) fn report(diagnostics: &mut DiagnosticBag, f: &FunctionNode<'_>, msg: String) {
    diagnostics.file_path = f.file_path.as_ref().map(|p| p.to_string());
    diagnostics.report_error(msg, Some(f.name.position));
}

pub(super) fn build_route(
    spec: RouteSpec<'_, '_>,
    cx: RouteCx<'_, '_>,
    diagnostics: &mut DiagnosticBag,
) -> Option<Route> {
    let RouteSpec {
        f,
        method,
        path,
        call,
        uses,
        websocket,
    } = spec;
    let RouteCx {
        fns,
        json_names,
        acc,
    } = cx;
    let placeholders = path_placeholders(path);
    let mut used_path: HashSet<String> = HashSet::new();
    let mut params = Vec::new();
    let mut has_body = false;
    let mut has_form = false;
    for p in &f.parameters {
        let ty = type_name(&p.type_);
        let name = p.name.text.clone();
        let kind = if ty == "HttpIncoming" {
            ParamKind::Incoming
        } else if ty == "RequestContext" {
            ParamKind::Context
        } else if ty == "ServerWebSocket" {
            if !websocket {
                report(
                    diagnostics,
                    f,
                    "ServerWebSocket is only valid on `@websocket` routes".to_string(),
                );
                return None;
            }
            ParamKind::ServerWs
        } else if let Some(dep) = attr_enum_name(&p.attributes, "dep") {
            if !fns.contains_key(&dep)
                && dep != "BearerToken"
                && dep != "ApiKeyHeader"
                && dep != "BasicAuth"
            {
                report(
                    diagnostics,
                    f,
                    format!("@dep({dep}) does not resolve to a function"),
                );
                return None;
            }
            ParamKind::Dep(dep)
        } else if has_attr(&p.attributes, "body") {
            if has_form {
                report(
                    diagnostics,
                    f,
                    "@body cannot be combined with @form/@file".to_string(),
                );
                return None;
            }
            let core = ty.strip_suffix("[]").unwrap_or(ty.as_str());
            if ty != "string" && ty != "byte[]" && ty != "JsonValue" && !json_names.contains(core) {
                report(
                    diagnostics,
                    f,
                    format!("@body parameter '{name}' must be @json, string, byte[], or JsonValue"),
                );
                return None;
            }
            has_body = true;
            ParamKind::Body
        } else if has_attr(&p.attributes, "form") {
            if has_body {
                report(
                    diagnostics,
                    f,
                    "@form cannot be combined with @body".to_string(),
                );
                return None;
            }
            has_form = true;
            let field = attr_string(&p.attributes, "form").unwrap_or_else(|| name.clone());
            ParamKind::Form(field)
        } else if has_attr(&p.attributes, "file") {
            if has_body {
                report(
                    diagnostics,
                    f,
                    "@file cannot be combined with @body".to_string(),
                );
                return None;
            }
            has_form = true;
            if ty != "UploadedFile" && option_inner(&ty) != Some("UploadedFile") {
                report(
                    diagnostics,
                    f,
                    format!("@file parameter '{name}' must be UploadedFile"),
                );
                return None;
            }
            let field = attr_string(&p.attributes, "file").unwrap_or_else(|| name.clone());
            ParamKind::File(field)
        } else if has_attr(&p.attributes, "query") {
            let q = attr_string(&p.attributes, "query").unwrap_or_else(|| name.clone());
            ParamKind::Query(q)
        } else if has_attr(&p.attributes, "header") {
            let h = attr_string(&p.attributes, "header").unwrap_or_else(|| name.clone());
            ParamKind::Header(h)
        } else if has_attr(&p.attributes, "cookie") {
            let c = attr_string(&p.attributes, "cookie").unwrap_or_else(|| name.clone());
            ParamKind::Cookie(c)
        } else {
            let key = attr_string(&p.attributes, "path").unwrap_or_else(|| name.clone());
            if !placeholders.iter().any(|ph| ph == &key) {
                report(
                    diagnostics,
                    f,
                    format!(
                        "parameter '{name}' is not a path segment of '{path}' and has no extractor"
                    ),
                );
                return None;
            }
            used_path.insert(key.clone());
            ParamKind::Path(key)
        };
        params.push(ParamPlan { name, ty, kind });
    }
    if websocket {
        let has_ws = params.iter().any(|p| matches!(p.kind, ParamKind::ServerWs));
        if !has_ws {
            report(
                diagnostics,
                f,
                "@websocket handler must take a ServerWebSocket parameter".to_string(),
            );
            return None;
        }
    }
    let mut dep_stack = Vec::new();
    for p in &params {
        if let ParamKind::Dep(dep) = &p.kind {
            if dep_has_cycle(dep, fns, acc, &mut dep_stack) {
                report(diagnostics, f, format!("@dep({dep}) forms a cycle"));
                return None;
            }
        }
    }
    for ph in &placeholders {
        if !used_path.contains(ph)
            && !params
                .iter()
                .any(|p| matches!(&p.kind, ParamKind::Path(n) if n == ph))
        {
            report(
                diagnostics,
                f,
                format!("path placeholder '{{{ph}}}' has no matching parameter"),
            );
            return None;
        }
    }
    Some(Route {
        method: method.to_string(),
        path: path.to_string(),
        fn_name: call.to_string(),
        is_async: f.is_async,
        ret: f
            .return_type
            .as_ref()
            .map(type_name)
            .unwrap_or_else(|| "void".into()),
        params,
        uses,
        websocket,
    })
}

pub(super) fn dep_has_cycle(
    name: &str,
    fns: &IndexMap<String, &FunctionNode<'_>>,
    acc: &ProgramAccumulator<'_>,
    stack: &mut Vec<String>,
) -> bool {
    if stack.iter().any(|s| s == name) {
        return true;
    }
    stack.push(name.to_string());
    let node = fns
        .get(name)
        .copied()
        .or_else(|| acc.all_functions.iter().find(|f| f.name.text == name));
    if let Some(f) = node {
        for p in &f.parameters {
            if let Some(nested) = attr_enum_name(&p.attributes, "dep") {
                if dep_has_cycle(&nested, fns, acc, stack) {
                    stack.pop();
                    return true;
                }
            }
        }
    }
    stack.pop();
    false
}

pub(super) fn resolve_routes(
    candidates: Vec<RouteCandidate<'_, '_>>,
    cx: RouteCx<'_, '_>,
    diagnostics: &mut DiagnosticBag,
) -> (Vec<Route>, bool) {
    let RouteCx {
        fns,
        json_names,
        acc,
    } = cx;
    let mut errors = false;
    let mut routes = Vec::new();
    let mut seen_keys = HashSet::new();
    for c in candidates {
        let (method, raw_path, websocket) = match &c.kind {
            RouteKind::Http { method, path } => (method.clone(), path.clone(), false),
            RouteKind::WebSocket { path } => ("GET".into(), path.clone(), true),
        };
        let path = join_http_path(&c.prefix, &raw_path);
        let key = (method.clone(), path.clone());
        if !seen_keys.insert(key) {
            report(
                diagnostics,
                c.f,
                format!("duplicate {} route '{}'", method, path),
            );
            errors = true;
            continue;
        }
        let mut uses = c.class_uses.clone();
        uses.extend(attr_enum_names(&c.f.attributes, "use"));
        if !validate_uses(&uses, c.f, fns, diagnostics) {
            errors = true;
            continue;
        }
        match build_route(
            RouteSpec {
                f: c.f,
                method: &method,
                path: &path,
                call: &c.call,
                uses,
                websocket,
            },
            RouteCx {
                fns,
                json_names,
                acc,
            },
            diagnostics,
        ) {
            Some(r) => routes.push(r),
            None => errors = true,
        }
    }
    (routes, errors)
}
