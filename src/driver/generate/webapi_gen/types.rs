use dream_syntax::nodes::Type;

pub(super) fn path_placeholders(path: &str) -> Vec<String> {
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

pub(super) fn type_name(t: &Type) -> String {
    match t {
        Type::Integer(_) => "int".into(),
        Type::UInt(_) => "uint".into(),
        Type::Long(_) => "long".into(),
        Type::ULong(_) => "ulong".into(),
        Type::ISize(_) => "isize".into(),
        Type::USize(_) => "usize".into(),
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

pub(super) fn option_inner(ty: &str) -> Option<&str> {
    ty.strip_prefix("Option<")?.strip_suffix(">")
}

pub(super) fn result_parts(ty: &str) -> Option<(&str, &str)> {
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
