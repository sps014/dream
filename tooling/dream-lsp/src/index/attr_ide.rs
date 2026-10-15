//! Attribute-aware IDE helpers: `@name` / `@name(...)` context detection, builtin completions,
//! hover text, and signature labels — all driven by [`dream_stdlib::attributes::all_specs`].

use super::is_ident_byte;
use dream_abi::attributes::{ArgKind, UserAttribute};
use dream_stdlib::attributes::{all_specs, attribute_catalog, find_spec};

/// Cursor is after `@` / `@partial` writing an attribute name (not inside `(...)` args).
pub fn attribute_name_partial(text: &str, offset: usize) -> Option<(usize, String)> {
    let bytes = text.as_bytes();
    let offset = offset.min(bytes.len());
    let mut i = offset;
    while i > 0 && is_ident_byte(bytes[i - 1]) {
        i -= 1;
    }
    if i == 0 || bytes[i - 1] != b'@' {
        return None;
    }
    // Reject `@` inside a string / char literal by a cheap scan for an odd number of
    // unescaped quotes on the same line before the `@`.
    if in_string_on_line(text, i - 1) {
        return None;
    }
    Some((i, text[i..offset].to_string()))
}

/// Cursor is inside `@name(...)` argument list.
#[derive(Debug, Clone)]
pub struct AttrArgContext {
    pub name: String,
    pub arg_index: usize,
    /// True when the cursor sits inside a `"..."` string argument.
    pub in_string: bool,
    pub qualified: bool,
}

pub fn attribute_arg_context(text: &str, offset: usize) -> Option<AttrArgContext> {
    let bytes = text.as_bytes();
    let offset = offset.min(bytes.len());

    let mut i = offset;
    let mut paren_depth = 0i32;
    let mut open_paren = None;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b')' => paren_depth += 1,
            b'(' => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                } else {
                    open_paren = Some(i);
                    break;
                }
            }
            b';' | b'{' | b'}' => return None,
            _ => {}
        }
    }
    let open_paren = open_paren?;

    // Name immediately before `(`: `@name(`
    let mut j = open_paren;
    while j > 0 && (bytes[j - 1] == b' ' || bytes[j - 1] == b'\t') {
        j -= 1;
    }
    let name_end = j;
    let mut name_start = name_end;
    while name_start > 0 && is_ident_byte(bytes[name_start - 1]) {
        name_start -= 1;
    }
    if name_start == name_end || name_start == 0 || bytes[name_start - 1] != b'@' {
        return None;
    }
    let name = text[name_start..name_end].to_string();

    let args_slice = &text[open_paren + 1..offset];
    let arg_index = count_args_before(args_slice);
    let in_string = string_open_at_end(args_slice);

    Some(AttrArgContext {
        name,
        arg_index,
        in_string,
        qualified: args_slice
            .rsplit(',')
            .next()
            .is_some_and(|arg| arg.contains('.')),
    })
}

fn count_args_before(slice: &str) -> usize {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    let mut commas = 0usize;
    for b in slice.bytes() {
        if in_str {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => commas += 1,
            _ => {}
        }
    }
    commas
}

fn string_open_at_end(slice: &str) -> bool {
    let mut in_str = false;
    let mut escape = false;
    for b in slice.bytes() {
        if in_str {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_str = false;
            }
        } else if b == b'"' {
            in_str = true;
        }
    }
    in_str
}

fn in_string_on_line(text: &str, at: usize) -> bool {
    let line_start = text[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
    string_open_at_end(&text[line_start..at])
}

/// Builtin attribute-name completion items: `(label, insert_text, detail, doc)`.
pub fn attribute_name_completions(partial: &str) -> Vec<(String, String, String, Option<String>)> {
    let partial_lower = partial.to_lowercase();
    let mut out = Vec::new();
    for spec in all_specs() {
        if !partial_lower.is_empty() && !spec.name.starts_with(&partial_lower) {
            // Also allow case-insensitive prefix match.
            if !spec.name.to_lowercase().starts_with(&partial_lower) {
                continue;
            }
        }
        let insert = if spec.required() == 0 {
            spec.name.to_string()
        } else {
            format!("{}($0)", spec.name)
        };
        out.push((
            spec.name.to_string(),
            insert,
            spec.signature_label(),
            Some(
                if dream_stdlib::BOOTSTRAP_PACKAGES.contains(&spec.module.as_str()) {
                    spec.doc.clone()
                } else {
                    format!(
                        "{}\n\nDeclared in `{}`; add `import {};`.",
                        spec.doc, spec.module, spec.module
                    )
                },
            ),
        ));
    }
    out
}

/// Closed-world argument suggestions for the attribute under the cursor.
/// Returns `(label, insert_text, detail, doc)`.
pub fn attribute_arg_completions(
    ctx: &AttrArgContext,
) -> Vec<(String, String, String, Option<String>)> {
    let Some(spec) = find_spec(&ctx.name) else {
        return Vec::new();
    };
    let Some(param) = spec
        .params
        .get(ctx.arg_index)
        .or_else(|| spec.params.last().filter(|p| p.variadic))
    else {
        return Vec::new();
    };
    if param.kind != ArgKind::Enum || ctx.in_string {
        return Vec::new();
    }
    let Some(enum_id) = &param.enum_type else {
        return Vec::new();
    };
    attribute_catalog()
        .enum_members(enum_id)
        .iter()
        .map(|member| {
            let value = if ctx.qualified {
                member.to_string()
            } else {
                format!("{}.{member}", param.ty)
            };
            (
                value.clone(),
                value,
                format!("@{} argument", ctx.name),
                None,
            )
        })
        .collect()
}

/// Synthetic detail + docs for attribute signature help, or `None` when not in an attribute call.
pub fn attribute_signature(text: &str, offset: usize) -> Option<(&'static UserAttribute, u32)> {
    let ctx = attribute_arg_context(text, offset)?;
    let spec = find_spec(&ctx.name)?;
    (!spec.params.is_empty()).then_some((spec, ctx.arg_index as u32))
}
