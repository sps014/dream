use dream_syntax::nodes::struct_node::StructDeclarationNode;
use dream_syntax::nodes::EnumDeclarationNode;

pub(super) const OK_MARKER: &str = "__DREAM_JSON_GEN_OK__";

pub(super) const ERR_MARKER: &str = "__DREAM_JSON_GEN_ERR__";

pub(super) const LOC_MARKER: &str = "__DREAM_JSON_GEN_LOC__";

pub(super) struct JsonGenError {
    pub(super) message: String,
    pub(super) type_name: Option<String>,
    pub(super) field_name: Option<String>,
}

pub(super) fn parse_generator_output(output: &str) -> Result<String, JsonGenError> {
    let trimmed = output.trim_start();
    if let Some(rest) = trimmed.strip_prefix(OK_MARKER) {
        let source = rest.strip_prefix('\n').unwrap_or(rest);
        return Ok(source.to_string());
    }
    if let Some(rest) = trimmed.strip_prefix(ERR_MARKER) {
        let body = rest.trim_start_matches('\n');
        let (msg_part, loc_part) = if let Some((m, l)) = body.split_once(LOC_MARKER) {
            (m.trim(), Some(l.trim()))
        } else {
            (body.trim(), None)
        };
        let message = if msg_part.is_empty() {
            "@json generator failed".to_string()
        } else {
            // Keep the first line as the user-facing message (LOC is separate).
            msg_part.lines().next().unwrap_or(msg_part).to_string()
        };
        let (type_name, field_name) = if let Some(loc) = loc_part {
            let mut parts = loc.splitn(2, '\t');
            let ty = parts.next().unwrap_or("").trim();
            let field = parts.next().unwrap_or("").trim();
            (
                if ty.is_empty() {
                    None
                } else {
                    Some(ty.to_string())
                },
                if field.is_empty() {
                    None
                } else {
                    Some(field.to_string())
                },
            )
        } else {
            (
                extract_quoted_after(msg_part, "class '")
                    .or_else(|| extract_quoted_after(msg_part, "union '")),
                extract_quoted_after(msg_part, "field '"),
            )
        };
        return Err(JsonGenError {
            message,
            type_name,
            field_name,
        });
    }
    Err(JsonGenError {
        message: format!("@json generator: unexpected harness output: {output}"),
        type_name: None,
        field_name: None,
    })
}

pub(super) fn extract_quoted_after(msg: &str, prefix: &str) -> Option<String> {
    let start = msg.find(prefix)? + prefix.len();
    let rest = &msg[start..];
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

pub(super) fn json_error_file_path(
    err: &JsonGenError,
    structs: &[StructDeclarationNode<'_>],
    enums: &[EnumDeclarationNode<'_>],
) -> Option<String> {
    let type_name = err.type_name.as_deref()?;
    for s in structs {
        if s.name.text == type_name {
            return s.file_path.as_ref().map(|p| p.to_string());
        }
    }
    for e in enums {
        if e.name.text == type_name {
            return e.file_path.as_ref().map(|p| p.to_string());
        }
    }
    None
}

pub(super) fn lookup_json_error_span(
    err: &JsonGenError,
    structs: &[StructDeclarationNode<'_>],
    enums: &[EnumDeclarationNode<'_>],
) -> Option<dream_text::text_span::TextSpan> {
    let type_name = err.type_name.as_deref()?;
    if let Some(field_name) = err.field_name.as_deref() {
        for s in structs {
            if s.name.text == type_name {
                for f in &s.fields {
                    if f.name.text == field_name {
                        return Some(f.name.position);
                    }
                }
                return Some(s.name.position);
            }
        }
        for e in enums {
            if e.name.text == type_name {
                for v in &e.variants {
                    for f in &v.fields {
                        if f.name.text == field_name {
                            return Some(f.name.position);
                        }
                    }
                }
                return Some(e.name.position);
            }
        }
    } else {
        for s in structs {
            if s.name.text == type_name {
                return Some(s.name.position);
            }
        }
        for e in enums {
            if e.name.text == type_name {
                return Some(e.name.position);
            }
        }
    }
    None
}
