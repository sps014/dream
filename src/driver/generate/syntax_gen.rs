//! Snapshot helpers + diagnostics for syntax-DSL generators that were not executed via
//! `@generator(ctx: GenContext)`. Markup/DSL logic lives in Dream generator bodies — not here.

use super::context::GeneratorContext;
use super::registration::RegisteredGenerator;
use super::syntax::SyntaxNodeId;
use dream_diagnostics::DiagnosticBag;
use std::collections::HashSet;

#[cfg(feature = "native")]
const OK_MARKER: &str = "__DREAM_GEN_OK__";
#[cfg(feature = "native")]
const ERR_MARKER: &str = "__DREAM_GEN_ERR__";

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
    format!("{{\"blocks\":{blocks}}}")
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
pub(super) enum HarnessError {
    Site { id: SyntaxNodeId, message: String },
    General(String),
}

#[cfg(feature = "native")]
pub(super) fn parse_harness_output(
    output: &str,
) -> Result<Vec<(SyntaxNodeId, String)>, HarnessError> {
    let trimmed = output.trim_start();
    if let Some(rest) = trimmed.strip_prefix(OK_MARKER) {
        let body = rest.trim_start_matches('\n');
        let mut out = Vec::new();
        for line in body.lines() {
            let line = line.trim();
            if line.is_empty() {
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
            out.push((SyntaxNodeId(id), expr.to_string()));
        }
        return Ok(out);
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
