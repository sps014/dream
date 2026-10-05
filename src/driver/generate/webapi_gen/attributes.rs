use super::model::RouteKind;
use dream_syntax::nodes::AttributeNode;

pub(super) const ROUTE_ATTRS: &[&str] =
    &["get", "post", "put", "patch", "delete", "head", "options"];

pub(super) fn is_std_file(path: Option<&str>) -> bool {
    path.is_none_or(dream_stdlib::is_std_source)
}

pub(super) fn has_attr(attrs: &[AttributeNode], name: &str) -> bool {
    attrs.iter().any(|a| a.name.text == name)
}

pub(super) fn route_kind(attrs: &[AttributeNode]) -> Option<RouteKind> {
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

pub(super) fn join_http_path(prefix: &str, path: &str) -> String {
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

pub(super) fn attr_int(attrs: &[AttributeNode], name: &str) -> Option<i32> {
    attrs
        .iter()
        .find(|a| a.name.text == name)?
        .args
        .first()?
        .as_int_text()?
        .parse()
        .ok()
}

pub(super) fn attr_string(attrs: &[AttributeNode], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|a| a.name.text == name)?
        .args
        .first()
        .and_then(|a| {
            a.as_string().map(|s| s.to_string()).or_else(|| {
                if let dream_syntax::nodes::AttributeArg::Enum(parts) = a {
                    Some(
                        parts
                            .iter()
                            .map(|t| t.text.as_str())
                            .collect::<Vec<_>>()
                            .join("."),
                    )
                } else {
                    None
                }
            })
        })
}

pub(super) fn attr_enum_names(attrs: &[AttributeNode], name: &str) -> Vec<String> {
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

pub(super) fn attr_enum_name(attrs: &[AttributeNode], name: &str) -> Option<String> {
    attr_enum_names(attrs, name).into_iter().next()
}
