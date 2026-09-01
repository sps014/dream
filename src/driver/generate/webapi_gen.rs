//! Builtin `system.webapi` dispatcher: `@get`/`@post`/… + extractors + `@dep` → `extend WebApp`.

use super::context::GeneratorContext;
use crate::driver::source_loader::ProgramAccumulator;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::function::FunctionNode;
use dream_syntax::nodes::types::CONSTRUCTOR_NAME;
use dream_syntax::nodes::{AttributeNode, Type};
use indexmap::IndexMap;
use std::collections::HashSet;

const ROUTE_ATTRS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

pub fn expand_from_acc(
    ctx: &mut GeneratorContext,
    acc: &ProgramAccumulator<'_>,
    diagnostics: &mut DiagnosticBag,
) {
    let wants_pkg = acc.requested_std_packages.contains("system.webapi");
    let mut has_routes = false;
    'detect: for f in &acc.all_functions {
        if route_kind(&f.attributes).is_some() || has_attr(&f.attributes, "middleware") {
            has_routes = true;
            break 'detect;
        }
    }
    if !has_routes {
        for s in &acc.all_structs {
            if is_std_file(s.file_path.as_deref()) {
                continue;
            }
            if has_attr(&s.attributes, "http_group") {
                has_routes = true;
                break;
            }
            for m in &s.methods {
                if route_kind(&m.attributes).is_some() {
                    has_routes = true;
                    break;
                }
            }
        }
    }
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

    let mut fns: IndexMap<String, &FunctionNode<'_>> = IndexMap::new();
    for f in &acc.all_functions {
        if is_std_file(f.file_path.as_deref()) {
            continue;
        }
        fns.entry(f.name.text.clone()).or_insert(f);
    }
    for s in &acc.all_structs {
        if is_std_file(s.file_path.as_deref()) {
            continue;
        }
        for m in &s.methods {
            fns.entry(m.name.text.clone()).or_insert(m);
        }
    }

    let mut errors = false;
    let mut routes: Vec<Route> = Vec::new();
    let mut seen_keys: HashSet<(String, String)> = HashSet::new();

    let mut candidates: Vec<RouteCandidate<'_>> = Vec::new();
    for f in &acc.all_functions {
        if is_std_file(f.file_path.as_deref()) {
            continue;
        }
        let Some(kind) = route_kind(&f.attributes) else {
            continue;
        };
        candidates.push(RouteCandidate {
            f,
            kind,
            prefix: String::new(),
            class_uses: Vec::new(),
            call: f.name.text.clone(),
        });
    }
    for s in &acc.all_structs {
        if is_std_file(s.file_path.as_deref()) {
            continue;
        }
        let prefix = attr_string(&s.attributes, "http_group").unwrap_or_default();
        let class_uses = attr_enum_names(&s.attributes, "use");
        for m in &s.methods {
            if m.name.text == CONSTRUCTOR_NAME {
                continue;
            }
            let Some(kind) = route_kind(&m.attributes) else {
                continue;
            };
            if !m.is_static {
                report(
                    diagnostics,
                    m,
                    "HTTP route methods on a class must be static".to_string(),
                );
                errors = true;
                continue;
            }
            candidates.push(RouteCandidate {
                f: m,
                kind,
                prefix: prefix.clone(),
                class_uses: class_uses.clone(),
                call: format!("{}.{}", s.name.text, m.name.text),
            });
        }
    }

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
        if !validate_uses(&uses, c.f, &fns, diagnostics) {
            errors = true;
            continue;
        }
        match build_route(
            c.f,
            &method,
            &path,
            &c.call,
            uses,
            websocket,
            &fns,
            &json_names,
            acc,
            diagnostics,
        ) {
            Some(r) => routes.push(r),
            None => errors = true,
        }
    }
    if errors {
        ctx.emit_extend("WebApp", stub_extend());
        return;
    }

    let mut middleware: Vec<(i32, String)> = Vec::new();
    for f in &acc.all_functions {
        if is_std_file(f.file_path.as_deref()) {
            continue;
        }
        if !has_attr(&f.attributes, "middleware") {
            continue;
        }
        let order = attr_int(&f.attributes, "middleware").unwrap_or(0);
        middleware.push((order, f.name.text.clone()));
    }
    middleware.sort_by_key(|(o, _)| *o);

    let source = emit_extend(&routes, &middleware, &json_names, acc);
    ctx.emit_extend("WebApp", source);
}

fn stub_extend() -> String {
    emit_extend(&[], &[], &HashSet::new(), &ProgramAccumulator::default())
}

fn is_std_file(path: Option<&str>) -> bool {
    path.map(|p| p.starts_with("<std>/"))
        .unwrap_or(true)
}

fn has_attr(attrs: &[AttributeNode], name: &str) -> bool {
    attrs.iter().any(|a| a.name.text == name)
}

fn route_kind(attrs: &[AttributeNode]) -> Option<RouteKind> {
    if let Some(a) = attrs.iter().find(|a| a.name.text == "websocket") {
        let path = a
            .args
            .first()
            .and_then(|x| x.as_string())
            .unwrap_or("/")
            .to_string();
        return Some(RouteKind::WebSocket { path });
    }
    for name in ROUTE_ATTRS {
        if let Some(a) = attrs.iter().find(|a| a.name.text == *name) {
            let path = a
                .args
                .first()
                .and_then(|x| x.as_string())
                .unwrap_or("/")
                .to_string();
            return Some(RouteKind::Http {
                method: name.to_uppercase(),
                path,
            });
        }
    }
    None
}

fn join_http_path(prefix: &str, path: &str) -> String {
    let p = prefix.trim_end_matches('/');
    let rest = if path.is_empty() {
        "/"
    } else if path.starts_with('/') {
        path
    } else {
        return if p.is_empty() {
            format!("/{path}")
        } else {
            format!("{p}/{path}")
        };
    };
    if p.is_empty() {
        rest.to_string()
    } else if rest == "/" {
        p.to_string()
    } else {
        format!("{p}{rest}")
    }
}

fn attr_int(attrs: &[AttributeNode], name: &str) -> Option<i32> {
    attrs
        .iter()
        .find(|a| a.name.text == name)?
        .args
        .first()?
        .as_int_text()?
        .parse()
        .ok()
}

fn attr_string(attrs: &[AttributeNode], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|a| a.name.text == name)?
        .args
        .first()
        .and_then(|a| a.as_string().map(|s| s.to_string()).or_else(|| {
            if let dream_syntax::nodes::AttributeArg::Enum(parts) = a {
                Some(parts.iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join("."))
            } else {
                None
            }
        }))
}

fn attr_enum_names(attrs: &[AttributeNode], name: &str) -> Vec<String> {
    let mut out = Vec::new();
    for a in attrs {
        if a.name.text != name {
            continue;
        }
        let Some(arg) = a.args.first() else {
            continue;
        };
        match arg {
            dream_syntax::nodes::AttributeArg::Enum(parts) => {
                out.push(
                    parts
                        .iter()
                        .map(|t| t.text.as_str())
                        .collect::<Vec<_>>()
                        .join("."),
                );
            }
            _ => {
                if let Some(s) = arg.as_string() {
                    out.push(s.to_string());
                }
            }
        }
    }
    out
}

fn is_middleware_shape(f: &FunctionNode<'_>) -> bool {
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

fn validate_uses(
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

fn attr_enum_name(attrs: &[AttributeNode], name: &str) -> Option<String> {
    attr_enum_names(attrs, name).into_iter().next()
}

fn report(diagnostics: &mut DiagnosticBag, f: &FunctionNode<'_>, msg: String) {
    diagnostics.file_path = f.file_path.as_ref().map(|p| p.to_string());
    diagnostics.report_error(msg, Some(f.name.position));
}

enum RouteKind {
    Http { method: String, path: String },
    WebSocket { path: String },
}

struct RouteCandidate<'a> {
    f: &'a FunctionNode<'a>,
    kind: RouteKind,
    prefix: String,
    class_uses: Vec<String>,
    call: String,
}

struct Route {
    method: String,
    path: String,
    fn_name: String,
    is_async: bool,
    ret: String,
    params: Vec<ParamPlan>,
    uses: Vec<String>,
    websocket: bool,
}

struct ParamPlan {
    name: String,
    ty: String,
    kind: ParamKind,
}

enum ParamKind {
    Incoming,
    Context,
    Path(String),
    Query(String),
    Header(String),
    Cookie(String),
    Body,
    Form(String),
    File(String),
    Dep(String),
    ServerWs,
}

fn path_placeholders(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        rest = &rest[start + 1..];
        if let Some(end) = rest.find('}') {
            out.push(rest[..end].to_string());
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    out
}

fn type_name(t: &Type) -> String {
    match t {
        Type::Integer(_) => "int".into(),
        Type::UInt(_) => "uint".into(),
        Type::Long(_) => "long".into(),
        Type::ULong(_) => "ulong".into(),
        Type::Byte(_) => "byte".into(),
        Type::Float(_) => "float".into(),
        Type::Double(_) => "double".into(),
        Type::Boolean(_) => "bool".into(),
        Type::Char(_) => "char".into(),
        Type::String(_) => "string".into(),
        Type::Void => "void".into(),
        Type::Object(_) => "object".into(),
        Type::Array(e) => format!("{}[]", type_name(e)),
        Type::Struct(tok, args) => {
            if let Some(args) = args {
                let inner = args.iter().map(type_name).collect::<Vec<_>>().join(", ");
                format!("{}<{}>", tok.text, inner)
            } else {
                tok.text.clone()
            }
        }
        Type::Generic(name) | Type::GenericFunctionItem(name) => name.clone(),
        Type::Unknown => "unknown".into(),
        Type::Tuple(elems) => {
            let inner = elems.iter().map(type_name).collect::<Vec<_>>().join(", ");
            format!("({inner})")
        }
        Type::Function(params, ret) => {
            let ps = params.iter().map(type_name).collect::<Vec<_>>().join(", ");
            format!("fun({ps}): {}", type_name(ret))
        }
    }
}

fn option_inner(ty: &str) -> Option<&str> {
    ty.strip_prefix("Option<")?.strip_suffix(">")
}

fn result_parts(ty: &str) -> Option<(&str, &str)> {
    let rest = ty.strip_prefix("Result<")?.strip_suffix(">")?;
    let mut depth = 0;
    for (i, c) in rest.char_indices() {
        if c == '<' {
            depth += 1;
        } else if c == '>' {
            depth -= 1;
        } else if c == ',' && depth == 0 {
            let ok = rest[..i].trim();
            let err = rest[i + 1..].trim();
            return Some((ok, err));
        }
    }
    None
}

fn build_route(
    f: &FunctionNode<'_>,
    method: &str,
    path: &str,
    call: &str,
    uses: Vec<String>,
    websocket: bool,
    fns: &IndexMap<String, &FunctionNode<'_>>,
    json_names: &HashSet<String>,
    acc: &ProgramAccumulator<'_>,
    diagnostics: &mut DiagnosticBag,
) -> Option<Route> {
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
            if !fns.contains_key(&dep) && dep != "BearerToken" && dep != "ApiKeyHeader" && dep != "BasicAuth"
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
                report(
                    diagnostics,
                    f,
                    format!("@dep({dep}) forms a cycle"),
                );
                return None;
            }
        }
    }
    for ph in &placeholders {
        if !used_path.contains(ph)
            && !params.iter().any(|p| matches!(&p.kind, ParamKind::Path(n) if n == ph))
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

fn dep_has_cycle(
    name: &str,
    fns: &IndexMap<String, &FunctionNode<'_>>,
    acc: &ProgramAccumulator<'_>,
    stack: &mut Vec<String>,
) -> bool {
    if stack.iter().any(|s| s == name) {
        return true;
    }
    stack.push(name.to_string());
    let node = fns.get(name).copied().or_else(|| {
        acc.all_functions.iter().find(|f| f.name.text == name)
    });
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

fn emit_extend(
    routes: &[Route],
    middleware: &[(i32, String)],
    json_names: &HashSet<String>,
    acc: &ProgramAccumulator<'_>,
) -> String {
    let mut s = String::new();
    s.push_str("    public static fun generated_install_middleware(): void {\n");
    s.push_str("        if !WebApp.begin_generated_middleware() {\n");
    s.push_str("            return;\n");
    s.push_str("        }\n");
    for (_, name) in middleware {
        s.push_str(&format!(
            "        WebApp.use(Middleware({name}));\n"
        ));
    }
    s.push_str("    }\n\n");
    s.push_str("    public static fun generated_openapi_paths(): string {\n");
    s.push_str("        return ");
    s.push_str(&json_string(&openapi_paths(routes, json_names, acc)));
    s.push_str(";\n    }\n\n");
    s.push_str(
        "    public static async fun generated_dispatch(ctx: RequestContext): HttpOutgoing {\n",
    );
    s.push_str("        let req = ctx.incoming;\n");
    s.push_str("        let method = req.method;\n");
    s.push_str("        let path = req.path;\n");
    for (i, r) in routes.iter().enumerate() {
        s.push_str(&format!(
            "        let __m{i} = WebApp.match_path(\"{}\", path);\n",
            escape_path(&r.path)
        ));
        s.push_str(&format!(
            "        if method == \"{}\" && __m{i}.is_some() {{\n",
            r.method
        ));
        s.push_str(&format!(
            "            let __params{i} = __m{i}.unwrap();\n"
        ));
        emit_handler_body(&mut s, r, i, json_names, acc);
        s.push_str("        }\n");
    }
    s.push_str("        return HttpOutgoing.not_found();\n");
    s.push_str("    }\n");
    s
}

fn escape_path(p: &str) -> String {
    p.replace('\\', "\\\\").replace('"', "\\\"")
}

fn json_string(raw: &str) -> String {
    let mut out = String::from("\"");
    for c in raw.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn emit_handler_body(
    s: &mut String,
    r: &Route,
    i: usize,
    json_names: &HashSet<String>,
    acc: &ProgramAccumulator<'_>,
) {
    let wrap = !r.uses.is_empty();
    let ind = if wrap { "                " } else { "            " };
    if wrap {
        s.push_str("            let __leaf: fun(): Future<HttpOutgoing> = async () => {\n");
    }
    emit_extractors(s, r, i, json_names, acc, ind);
    if wrap {
        s.push_str("            };\n");
        s.push_str("            let __uses = List<Middleware>();\n");
        for u in &r.uses {
            s.push_str(&format!("            __uses.push(Middleware({u}));\n"));
        }
        s.push_str(
            "            return await WebApp.run_local_middleware(ctx, __uses, 0, __leaf);\n",
        );
    }
}

fn emit_extractors(
    s: &mut String,
    r: &Route,
    i: usize,
    json_names: &HashSet<String>,
    acc: &ProgramAccumulator<'_>,
    ind: &str,
) {
    let mut args: Vec<String> = Vec::new();
    let mut dep_memo: HashSet<String> = HashSet::new();
    let needs_multipart = r
        .params
        .iter()
        .any(|p| matches!(p.kind, ParamKind::Form(_) | ParamKind::File(_)));
    if needs_multipart {
        s.push_str(&format!(
            "{ind}if WebApp.host_parse_multipart(req.req_id) != 1 {{ return HttpOutgoing.from_status(HttpStatus(400, \"expected multipart/form-data\")); }}\n"
        ));
    }
    for p in &r.params {
        match &p.kind {
            ParamKind::Incoming => {
                s.push_str(&format!("{ind}let {} = req;\n", p.name));
                args.push(p.name.clone());
            }
            ParamKind::Context => {
                s.push_str(&format!("{ind}let {} = ctx;\n", p.name));
                args.push(p.name.clone());
            }
            ParamKind::ServerWs => {
                s.push_str(&format!(
                    "{ind}let __ws_{} = ServerWebSocket.upgrade(req);\n",
                    p.name
                ));
                s.push_str(&format!(
                    "{ind}if __ws_{}.is_none() {{ return HttpOutgoing.from_status(HttpStatus(400, \"websocket upgrade failed\")); }}\n",
                    p.name
                ));
                s.push_str(&format!(
                    "{ind}let {} = __ws_{}.unwrap();\n",
                    p.name, p.name
                ));
                args.push(p.name.clone());
            }
            ParamKind::Path(key) => {
                s.push_str(&format!(
                    "{ind}let __p_{} = __params{i}.get(\"{key}\").unwrap_or(string.empty);\n",
                    p.name
                ));
                emit_parse_scalar(s, ind, &p.name, &p.ty, &format!("__p_{}", p.name));
                args.push(p.name.clone());
            }
            ParamKind::Query(key) => {
                s.push_str(&format!(
                    "{ind}let __q_{} = req.query_param(\"{key}\");\n",
                    p.name
                ));
                if option_inner(&p.ty).is_some() {
                    s.push_str(&format!("{ind}let {} = __q_{};\n", p.name, p.name));
                } else {
                    s.push_str(&format!(
                        "{ind}if __q_{}.is_none() {{ return HttpOutgoing.from_status(HttpStatus(400,\"missing query {key}\")); }}\n",
                        p.name
                    ));
                    emit_parse_scalar(
                        s,
                        ind,
                        &p.name,
                        &p.ty,
                        &format!("__q_{}.unwrap_or(string.empty)", p.name),
                    );
                }
                args.push(p.name.clone());
            }
            ParamKind::Header(key) => {
                s.push_str(&format!(
                    "{ind}let __h_{} = req.header(\"{key}\");\n",
                    p.name
                ));
                if option_inner(&p.ty).is_some() {
                    s.push_str(&format!("{ind}let {} = __h_{};\n", p.name, p.name));
                } else {
                    s.push_str(&format!(
                        "{ind}if __h_{}.is_none() {{ return HttpOutgoing.from_status(HttpStatus(400,\"missing header {key}\")); }}\n",
                        p.name
                    ));
                    s.push_str(&format!(
                        "{ind}let {} = __h_{}.unwrap_or(string.empty);\n",
                        p.name, p.name
                    ));
                }
                args.push(p.name.clone());
            }
            ParamKind::Cookie(key) => {
                s.push_str(&format!(
                    "{ind}let __c_{} = req.cookie(\"{key}\");\n",
                    p.name
                ));
                if option_inner(&p.ty).is_some() {
                    s.push_str(&format!("{ind}let {} = __c_{};\n", p.name, p.name));
                } else {
                    s.push_str(&format!(
                        "{ind}if __c_{}.is_none() {{ return HttpOutgoing.from_status(HttpStatus(400,\"missing cookie {key}\")); }}\n",
                        p.name
                    ));
                    s.push_str(&format!(
                        "{ind}let {} = __c_{}.unwrap_or(string.empty);\n",
                        p.name, p.name
                    ));
                }
                args.push(p.name.clone());
            }
            ParamKind::Body => {
                s.push_str(&format!("{ind}let __body_{} = req.read_body_text();\n", p.name));
                if p.ty == "string" {
                    s.push_str(&format!("{ind}let {} = __body_{};\n", p.name, p.name));
                } else if p.ty == "byte[]" {
                    s.push_str(&format!(
                        "{ind}let {} = req.read_body_bytes();\n",
                        p.name
                    ));
                } else {
                    s.push_str(&format!(
                        "{ind}let __bj_{} = Json.deserialize<{}>(__body_{});\n",
                        p.name, p.ty, p.name
                    ));
                    s.push_str(&format!(
                        "{ind}if __bj_{}.is_err() {{ return HttpOutgoing.from_status(HttpStatus(400,\"invalid json\")); }}\n",
                        p.name
                    ));
                    s.push_str(&format!(
                        "{ind}let {} = __bj_{}.unwrap();\n",
                        p.name, p.name
                    ));
                }
                args.push(p.name.clone());
            }
            ParamKind::Form(key) => {
                s.push_str(&format!(
                    "{ind}let __f_{} = WebApp.multipart_field(req.req_id, \"{key}\");\n",
                    p.name
                ));
                if option_inner(&p.ty).is_some() {
                    s.push_str(&format!("{ind}let {} = __f_{};\n", p.name, p.name));
                } else {
                    s.push_str(&format!(
                        "{ind}if __f_{}.is_none() {{ return HttpOutgoing.from_status(HttpStatus(400,\"missing form {key}\")); }}\n",
                        p.name
                    ));
                    s.push_str(&format!(
                        "{ind}let {} = __f_{}.unwrap_or(string.empty);\n",
                        p.name, p.name
                    ));
                }
                args.push(p.name.clone());
            }
            ParamKind::File(key) => {
                s.push_str(&format!(
                    "{ind}let __file_{} = WebApp.multipart_file(req.req_id, \"{key}\");\n",
                    p.name
                ));
                if option_inner(&p.ty).is_some() {
                    s.push_str(&format!("{ind}let {} = __file_{};\n", p.name, p.name));
                } else {
                    s.push_str(&format!(
                        "{ind}if __file_{}.is_none() {{ return HttpOutgoing.from_status(HttpStatus(400,\"missing file {key}\")); }}\n",
                        p.name
                    ));
                    s.push_str(&format!(
                        "{ind}let {} = __file_{}.unwrap();\n",
                        p.name, p.name
                    ));
                }
                args.push(p.name.clone());
            }
            ParamKind::Dep(dep) => {
                emit_dep_call(s, ind, &p.name, &p.ty, dep, acc, &mut dep_memo);
                args.push(p.name.clone());
            }
        }
    }
    let call_args = args.join(", ");
    let call = if r.is_async {
        format!("await {}({call_args})", r.fn_name)
    } else {
        format!("{}({call_args})", r.fn_name)
    };
    if r.websocket {
        s.push_str(&format!("{ind}{call};\n"));
        s.push_str(&format!("{ind}return HttpOutgoing.already_sent();\n"));
        return;
    }
    emit_return(s, ind, &r.ret, &call, json_names);
}

fn emit_parse_scalar(s: &mut String, ind: &str, name: &str, ty: &str, src: &str) {
    match ty {
        "string" => s.push_str(&format!("{ind}let {name} = {src};\n")),
        "int" => {
            s.push_str(&format!("{ind}let __pi_{name} = int.parse({src});\n"));
            s.push_str(&format!(
                "{ind}if __pi_{name}.is_err() {{ return HttpOutgoing.from_status(HttpStatus(400,\"invalid {name}\")); }}\n"
            ));
            s.push_str(&format!("{ind}let {name} = __pi_{name}.unwrap();\n"));
        }
        "bool" => {
            s.push_str(&format!(
                "{ind}let {name} = {src} == \"true\" || {src} == \"1\";\n"
            ));
        }
        _ => s.push_str(&format!("{ind}let {name} = {src};\n")),
    }
}

fn emit_dep_call(
    s: &mut String,
    ind: &str,
    bind: &str,
    ty: &str,
    dep: &str,
    acc: &ProgramAccumulator<'_>,
    memo: &mut HashSet<String>,
) {
    let slot = format!("__dep_{dep}");
    if memo.contains(dep) {
        s.push_str(&format!("{ind}let {bind} = {slot};\n"));
        return;
    }
    memo.insert(dep.to_string());
    let dep_fn = acc.all_functions.iter().find(|f| f.name.text == dep);
    let mut dep_args: Vec<String> = Vec::new();
    if let Some(df) = dep_fn {
        for (j, p) in df.parameters.iter().enumerate() {
            let pty = type_name(&p.type_);
            let tmp = format!("__d_{bind}_{j}");
            if let Some(nested) = attr_enum_name(&p.attributes, "dep") {
                emit_dep_call(s, ind, &tmp, &pty, &nested, acc, memo);
                dep_args.push(tmp);
            } else if has_attr(&p.attributes, "header") {
                let h = attr_string(&p.attributes, "header").unwrap_or_else(|| p.name.text.clone());
                s.push_str(&format!("{ind}let {tmp}_o = req.header(\"{h}\");\n"));
                if option_inner(&pty).is_some() {
                    s.push_str(&format!("{ind}let {tmp} = {tmp}_o;\n"));
                } else {
                    s.push_str(&format!(
                        "{ind}if {tmp}_o.is_none() {{ return HttpOutgoing.from_status(HttpStatus.unauthorized()); }}\n"
                    ));
                    s.push_str(&format!("{ind}let {tmp} = {tmp}_o.unwrap_or(string.empty);\n"));
                }
                dep_args.push(tmp);
            } else if has_attr(&p.attributes, "query") {
                let q = attr_string(&p.attributes, "query").unwrap_or_else(|| p.name.text.clone());
                s.push_str(&format!("{ind}let {tmp} = req.query_param(\"{q}\").unwrap_or(string.empty);\n"));
                dep_args.push(tmp);
            } else if pty == "HttpIncoming" {
                dep_args.push("req".into());
            } else if pty == "RequestContext" {
                s.push_str(&format!("{ind}let {tmp} = RequestContext(req);\n"));
                dep_args.push(tmp);
            } else {
                s.push_str(&format!(
                    "{ind}return HttpOutgoing.from_status(HttpStatus(500, \"unsupported dependency parameter\"));\n"
                ));
                return;
            }
        }
    }
    let args = dep_args.join(", ");
    let call = if dep_fn.map(|f| f.is_async).unwrap_or(true) {
        format!("await {dep}({args})")
    } else {
        format!("{dep}({args})")
    };
    if let Some((_, err)) = result_parts(ty) {
        if err == "HttpStatus" {
            s.push_str(&format!("{ind}let __dr_{bind} = {call};\n"));
            s.push_str(&format!(
                "{ind}if __dr_{bind}.is_err() {{ return HttpOutgoing.from_status(__dr_{bind}.unwrap_err()); }}\n"
            ));
            s.push_str(&format!("{ind}let {slot} = __dr_{bind}.unwrap();\n"));
            s.push_str(&format!("{ind}let {bind} = {slot};\n"));
            return;
        }
    }
    let dep_ret = dep_fn
        .and_then(|f| f.return_type.as_ref())
        .map(type_name)
        .unwrap_or_else(|| ty.to_string());
    if let Some((ok, err)) = result_parts(&dep_ret) {
        if err == "HttpStatus" {
            s.push_str(&format!("{ind}let __dr_{bind} = {call};\n"));
            s.push_str(&format!(
                "{ind}if __dr_{bind}.is_err() {{ return HttpOutgoing.from_status(__dr_{bind}.unwrap_err()); }}\n"
            ));
            s.push_str(&format!("{ind}let {slot} = __dr_{bind}.unwrap();\n"));
            s.push_str(&format!("{ind}let {bind} = {slot};\n"));
            let _ = ok;
            return;
        }
    }
    s.push_str(&format!("{ind}let {slot} = {call};\n"));
    s.push_str(&format!("{ind}let {bind} = {slot};\n"));
}

fn emit_return(
    s: &mut String,
    ind: &str,
    ret: &str,
    call: &str,
    json_names: &HashSet<String>,
) {
    if ret == "HttpOutgoing" {
        s.push_str(&format!("{ind}return {call};\n"));
        return;
    }
    if ret == "void" {
        s.push_str(&format!("{ind}{call};\n"));
        s.push_str(&format!("{ind}return HttpOutgoing.empty(204);\n"));
        return;
    }
    if ret == "HttpStatus" {
        s.push_str(&format!("{ind}return HttpOutgoing.from_status({call});\n"));
        return;
    }
    if ret == "EventStream" {
        s.push_str(&format!("{ind}let __es = {call};\n"));
        s.push_str(&format!("{ind}__es.end();\n"));
        s.push_str(&format!("{ind}return HttpOutgoing.already_sent();\n"));
        return;
    }
    s.push_str(&format!("{ind}let __out = {call};\n"));
    if let Some((ok, err)) = result_parts(ret) {
        if err == "HttpStatus" {
            s.push_str(&format!(
                "{ind}if __out.is_err() {{ return HttpOutgoing.from_status(__out.unwrap_err()); }}\n"
            ));
            s.push_str(&format!("{ind}let __ok = __out.unwrap();\n"));
            emit_value_return(s, ind, ok, "__ok", json_names);
            return;
        }
        if err == "string" {
            s.push_str(&format!(
                "{ind}if __out.is_err() {{ return HttpOutgoing.detail(__out.unwrap_err(), 500); }}\n"
            ));
        } else {
            s.push_str(&format!(
                "{ind}if __out.is_err() {{ return HttpOutgoing.detail(__out.unwrap_err().to_string(), 500); }}\n"
            ));
        }
        s.push_str(&format!("{ind}let __ok = __out.unwrap();\n"));
        emit_value_return(s, ind, ok, "__ok", json_names);
        return;
    }
    emit_value_return(s, ind, ret, "__out", json_names);
}

fn emit_value_return(
    s: &mut String,
    ind: &str,
    ty: &str,
    expr: &str,
    json_names: &HashSet<String>,
) {
    if ty == "string" {
        s.push_str(&format!("{ind}return HttpOutgoing.text({expr}, 200);\n"));
    } else if ty == "HttpOutgoing" {
        s.push_str(&format!("{ind}return {expr};\n"));
    } else if json_names.contains(ty) || ty == "JsonValue" {
        s.push_str(&format!(
            "{ind}return HttpOutgoing.json_text(Json.serialize({expr}), 200);\n"
        ));
    } else {
        s.push_str(&format!(
            "{ind}return HttpOutgoing.json_text(Json.serialize({expr}), 200);\n"
        ));
    }
}

fn openapi_paths(
    routes: &[Route],
    json_names: &HashSet<String>,
    acc: &ProgramAccumulator<'_>,
) -> String {
    let mut s = String::from("{");
    let mut first_path = true;
    let mut by_path: IndexMap<String, Vec<&Route>> = IndexMap::new();
    for r in routes {
        by_path.entry(r.path.clone()).or_default().push(r);
    }
    for (path, rs) in &by_path {
        if !first_path {
            s.push(',');
        }
        first_path = false;
        s.push_str(&json_raw_string(path));
        s.push_str(":{");
        let mut first_m = true;
        for r in rs {
            if !first_m {
                s.push(',');
            }
            first_m = false;
            s.push_str(&json_raw_string(&r.method.to_lowercase()));
            s.push_str(":{\"operationId\":");
            s.push_str(&json_raw_string(&r.fn_name));
            let mut params_json = String::new();
            let mut first_p = true;
            params_json.push('[');
            let mut body: Option<String> = None;
            let mut form_props = String::new();
            let mut first_form = true;
            for p in &r.params {
                match &p.kind {
                    ParamKind::Path(n) => {
                        if !first_p {
                            params_json.push(',');
                        }
                        first_p = false;
                        params_json.push_str("{\"name\":");
                        params_json.push_str(&json_raw_string(n));
                        params_json.push_str(",\"in\":\"path\",\"required\":true,\"schema\":{\"type\":");
                        params_json.push_str(&json_raw_string(openapi_scalar(&p.ty)));
                        params_json.push_str("}}");
                    }
                    ParamKind::Query(n) | ParamKind::Header(n) => {
                        if !first_p {
                            params_json.push(',');
                        }
                        first_p = false;
                        let loc = if matches!(&p.kind, ParamKind::Query(_)) {
                            "query"
                        } else {
                            "header"
                        };
                        params_json.push_str("{\"name\":");
                        params_json.push_str(&json_raw_string(n));
                        params_json.push_str(",\"in\":");
                        params_json.push_str(&json_raw_string(loc));
                        params_json.push_str(",\"required\":");
                        params_json.push_str(if option_inner(&p.ty).is_some() {
                            "false"
                        } else {
                            "true"
                        });
                        params_json.push_str(",\"schema\":{\"type\":\"string\"}}");
                    }
                    ParamKind::Body => {
                        body = Some(openapi_schema_str(&p.ty, json_names, acc));
                    }
                    ParamKind::Form(n) => {
                        if !first_form {
                            form_props.push(',');
                        }
                        first_form = false;
                        form_props.push_str(&json_raw_string(n));
                        form_props.push_str(":{\"type\":\"string\"}");
                    }
                    ParamKind::File(n) => {
                        if !first_form {
                            form_props.push(',');
                        }
                        first_form = false;
                        form_props.push_str(&json_raw_string(n));
                        form_props.push_str(":{\"type\":\"string\",\"format\":\"binary\"}");
                    }
                    _ => {}
                }
            }
            params_json.push(']');
            if params_json != "[]" {
                s.push_str(",\"parameters\":");
                s.push_str(&params_json);
            }
            if let Some(schema) = body {
                s.push_str(",\"requestBody\":{\"required\":true,\"content\":{\"application/json\":{\"schema\":");
                s.push_str(&schema);
                s.push_str("}}}");
            } else if !form_props.is_empty() {
                s.push_str(",\"requestBody\":{\"required\":true,\"content\":{\"multipart/form-data\":{\"schema\":{\"type\":\"object\",\"properties\":{");
                s.push_str(&form_props);
                s.push_str("}}}}}");
            }
            s.push_str(",\"responses\":{\"200\":{\"description\":\"OK\"}}}");
        }
        s.push('}');
    }
    s.push('}');
    s
}

fn json_raw_string(s: &str) -> String {
    json_string(s)
}

fn openapi_schema_str(
    ty: &str,
    json_names: &HashSet<String>,
    acc: &ProgramAccumulator<'_>,
) -> String {
    if json_names.contains(ty) {
        if let Some(st) = acc.all_structs.iter().find(|s| s.name.text == ty) {
            let mut s = String::from("{\"type\":\"object\",\"properties\":{");
            let mut first = true;
            let mut req = String::from("[");
            let mut first_r = true;
            for f in &st.fields {
                if !first {
                    s.push(',');
                }
                first = false;
                s.push_str(&json_raw_string(&f.name.text));
                s.push_str(":{\"type\":");
                s.push_str(&json_raw_string(openapi_scalar(&type_name(&f.field_type))));
                s.push('}');
                if !first_r {
                    req.push(',');
                }
                first_r = false;
                req.push_str(&json_raw_string(&f.name.text));
            }
            req.push(']');
            s.push_str("},\"required\":");
            s.push_str(&req);
            s.push('}');
            return s;
        }
    }
    "{\"type\":\"object\"}".to_string()
}

fn openapi_scalar(ty: &str) -> &'static str {
    match ty {
        "int" | "uint" | "long" | "ulong" | "byte" => "integer",
        "float" | "double" => "number",
        "bool" => "boolean",
        _ => "string",
    }
}
