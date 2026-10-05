use super::analysis::report;
use super::attributes::attr_enum_names;
use super::attributes::attr_int;
use super::attributes::attr_string;
use super::attributes::has_attr;
use super::attributes::is_std_file;
use super::attributes::route_kind;
use super::model::RouteCandidate;
use crate::driver::source_loader::ProgramAccumulator;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::function::FunctionNode;
use dream_syntax::nodes::types::CONSTRUCTOR_NAME;
use indexmap::IndexMap;

pub(super) fn has_routes(acc: &ProgramAccumulator<'_>) -> bool {
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
    has_routes
}

pub(super) fn collect_functions<'a, 'src>(
    acc: &'a ProgramAccumulator<'src>,
) -> IndexMap<String, &'a FunctionNode<'src>> {
    let mut fns: IndexMap<String, &FunctionNode<'src>> = IndexMap::new();
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

    fns
}

pub(super) fn collect_candidates<'a, 'src>(
    acc: &'a ProgramAccumulator<'src>,
    diagnostics: &mut DiagnosticBag,
) -> (Vec<RouteCandidate<'a, 'src>>, bool) {
    let mut errors = false;
    let mut candidates: Vec<RouteCandidate<'_, '_>> = Vec::new();
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

    (candidates, errors)
}

pub(super) fn collect_middleware(acc: &ProgramAccumulator<'_>) -> Vec<(i32, String)> {
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

    middleware
}
