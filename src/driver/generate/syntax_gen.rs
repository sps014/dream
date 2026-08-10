//! Snapshot helpers + diagnostics for syntax-DSL generators that were not executed via
//! `@generator(ctx: GenContext)`. Markup/DSL logic lives in Dream generator bodies — not here.

use super::context::GeneratorContext;
use super::registration::RegisteredGenerator;
#[cfg(feature = "native")]
use super::semantic::TypeSymbol;
#[cfg(feature = "native")]
use super::syntax::SyntaxNodeId;
use dream_diagnostics::DiagnosticBag;
use std::collections::HashSet;

#[cfg(feature = "native")]
const OK_MARKER: &str = "__DREAM_GEN_OK__";
#[cfg(feature = "native")]
const ERR_MARKER: &str = "__DREAM_GEN_ERR__";
#[cfg(feature = "native")]
const EMIT_EXTEND_MARKER: &str = "__DREAM_GEN_EMIT_EXTEND__";
#[cfg(feature = "native")]
const EMIT_FILE_MARKER: &str = "__DREAM_GEN_EMIT_FILE__";

/// Reports errors for registered `@syntax_block` generators that were not run by
/// `context_gen` (missing `GenContext` body). Sibling `harness.dream` is no longer supported.
pub fn expand_syntax_blocks(
    ctx: &mut GeneratorContext,
    diagnostics: &mut DiagnosticBag,
    handled: &HashSet<String>,
) {
    let gens: Vec<RegisteredGenerator> = ctx
        .registered
        .iter()
        .filter(|g| !g.syntax_blocks.is_empty() && !handled.contains(&g.name))
        .cloned()
        .collect();
    if gens.is_empty() {
        return;
    }

    for gen in gens {
        let mut has_sites = false;
        for name in &gen.syntax_blocks {
            if !ctx.syntax_blocks(name).is_empty() {
                has_sites = true;
                break;
            }
        }
        if !has_sites {
            continue;
        }
        diagnostics.report_error(
            format!(
                "syntax generator '{}': @generator must take a single GenContext parameter and have a non-empty body",
                gen.name
            ),
            None,
        );
    }
}

#[cfg(feature = "native")]
pub(super) fn build_snapshot(ctx: &GeneratorContext, site_ids: &[SyntaxNodeId]) -> String {
    let mut types = String::from("[");
    let mut first_type = true;
    for t in ctx.types() {
        if !first_type {
            types.push(',');
        }
        first_type = false;
        types.push_str(&snapshot_type(t));
    }
    types.push(']');

    let mut blocks = String::from("[");
    let mut first = true;
    for id in site_ids {
        let Some(site) = ctx.syntax.block_keys.get(id) else {
            continue;
        };
        if !first {
            blocks.push(',');
        }
        first = false;
        blocks.push_str("{\"id\":");
        blocks.push_str(&id.0.to_string());
        blocks.push_str(",\"name\":");
        blocks.push_str(&json_escape(&site.name));
        blocks.push_str(",\"body\":");
        blocks.push_str(&json_escape(&site.body_text));
        blocks.push_str(",\"splices\":");
        blocks.push_str(&json_string_array(&site.splice_sources));
        blocks.push('}');
    }
    blocks.push(']');
    format!("{{\"types\":{types},\"blocks\":{blocks}}}")
}

#[cfg(feature = "native")]
fn snapshot_type(t: &TypeSymbol) -> String {
    let attr_names: Vec<String> = t.attributes.iter().map(|a| a.name.clone()).collect();
    let mut fields = String::from("[");
    for (i, f) in t.fields.iter().enumerate() {
        if i > 0 {
            fields.push(',');
        }
        fields.push_str("{\"name\":");
        fields.push_str(&json_escape(&f.name));
        fields.push_str(",\"type_name\":");
        fields.push_str(&json_escape(&f.type_name));
        fields.push('}');
    }
    fields.push(']');
    format!(
        "{{\"name\":{},\"attributes\":{},\"fields\":{}}}",
        json_escape(&t.name),
        json_string_array(&attr_names),
        fields,
    )
}

#[cfg(feature = "native")]
fn json_string_array(items: &[String]) -> String {
    let mut out = String::from("[");
    for (i, s) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&json_escape(s));
    }
    out.push(']');
    out
}

#[cfg(feature = "native")]
fn json_escape(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(feature = "native")]
#[derive(Debug)]
pub(super) enum HarnessError {
    Site { id: SyntaxNodeId, message: String },
    General(String),
}

#[cfg(feature = "native")]
pub(super) struct HarnessOutput {
    pub replacements: Vec<(SyntaxNodeId, String)>,
    pub extend_emits: Vec<(String, String)>,
    pub file_emits: Vec<(String, String)>,
}

#[cfg(feature = "native")]
pub(super) fn parse_harness_output(output: &str) -> Result<HarnessOutput, HarnessError> {
    let trimmed = output.trim_start();
    if let Some(rest) = trimmed.strip_prefix(OK_MARKER) {
        let body = rest.trim_start_matches('\n');
        let mut replacements = Vec::new();
        let mut extend_emits = Vec::new();
        let mut file_emits = Vec::new();
        let mut lines = body.lines().map(str::trim).filter(|l| !l.is_empty());
        while let Some(line) = lines.next() {
            if line == EMIT_EXTEND_MARKER {
                let type_name = lines.next().ok_or_else(|| {
                    HarnessError::General(
                        "syntax generator: emit_extend missing type name".to_string(),
                    )
                })?;
                let body_line = lines.next().ok_or_else(|| {
                    HarnessError::General(
                        "syntax generator: emit_extend missing body line".to_string(),
                    )
                })?;
                extend_emits.push((type_name.to_string(), unescape_emit_body(body_line)));
                continue;
            }
            if line == EMIT_FILE_MARKER {
                let path = lines.next().ok_or_else(|| {
                    HarnessError::General("syntax generator: emit_file missing path".to_string())
                })?;
                let source_line = lines.next().ok_or_else(|| {
                    HarnessError::General("syntax generator: emit_file missing source".to_string())
                })?;
                file_emits.push((path.to_string(), unescape_emit_body(source_line)));
                continue;
            }
            let Some((id_str, expr)) = line.split_once('\t') else {
                return Err(HarnessError::General(format!(
                    "syntax generator: bad OK line (expected id\\texpr): {line}"
                )));
            };
            let id: u32 = id_str.parse().map_err(|_| {
                HarnessError::General(format!("syntax generator: bad node id '{id_str}'"))
            })?;
            replacements.push((SyntaxNodeId(id), expr.to_string()));
        }
        return Ok(HarnessOutput {
            replacements,
            extend_emits,
            file_emits,
        });
    }
    if let Some(rest) = trimmed.strip_prefix(ERR_MARKER) {
        let body = rest.trim_start_matches('\n').trim();
        let first = body.lines().next().unwrap_or(body);
        if let Some((id_str, message)) = first.split_once('\t') {
            if let Ok(id) = id_str.parse::<u32>() {
                return Err(HarnessError::Site {
                    id: SyntaxNodeId(id),
                    message: message.to_string(),
                });
            }
        }
        let message = if first.is_empty() {
            "syntax generator failed".to_string()
        } else {
            first.to_string()
        };
        return Err(HarnessError::General(message));
    }
    Err(HarnessError::General(format!(
        "syntax generator: unexpected output: {output}"
    )))
}

#[cfg(feature = "native")]
fn unescape_emit_body(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(all(feature = "native", test))]
mod tests {
    use super::*;

    #[test]
    fn parse_ok_replacements_and_emits() {
        let output = "__DREAM_GEN_OK__\n\
1\t\"a\"\n\
__DREAM_GEN_EMIT_EXTEND__\n\
Point\n\
public fun describe(): string {\\n    return \"dto\";\\n}\n\
__DREAM_GEN_EMIT_FILE__\n\
gen.dream\n\
extend Foo {\\n}\n";
        let parsed = parse_harness_output(output).unwrap();
        assert_eq!(parsed.replacements.len(), 1);
        assert_eq!(parsed.replacements[0].0, SyntaxNodeId(1));
        assert_eq!(parsed.replacements[0].1, "\"a\"");
        assert_eq!(parsed.extend_emits.len(), 1);
        assert_eq!(parsed.extend_emits[0].0, "Point");
        assert!(parsed.extend_emits[0].1.contains("return \"dto\""));
        assert_eq!(parsed.file_emits.len(), 1);
        assert_eq!(parsed.file_emits[0].0, "gen.dream");
        assert!(parsed.file_emits[0].1.contains("extend Foo"));
    }
}
