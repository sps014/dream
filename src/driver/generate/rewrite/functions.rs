use super::model::Rewriter;
use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::function::FunctionNode;
use dream_syntax::nodes::StatementNode;
use indexmap::IndexMap;
use std::io::Error;

/// Rebuilds a function body, replacing syntax blocks whose site key is in `by_site`.
pub fn rewrite_function_body<'a>(
    arena: &'a Bump,
    body: &'a [StatementNode<'a>],
    by_site: &IndexMap<(String, String), String>,
    diagnostics: &mut DiagnosticBag,
    file: Option<&str>,
    file_contents: &std::collections::HashMap<String, String>,
) -> Result<&'a [StatementNode<'a>], Error> {
    let mut changed = false;
    let mut rewriter = Rewriter {
        arena,
        by_site,
        diagnostics,
        changed: &mut changed,
        file,
        file_contents,
    };
    let mut out = Vec::with_capacity(body.len());
    for s in body {
        out.push(rewriter.statement(s)?);
    }
    if changed {
        Ok(arena.alloc_slice_fill_iter(out))
    } else {
        Ok(body)
    }
}

pub fn rewrite_function<'a>(
    arena: &'a Bump,
    f: &mut FunctionNode<'a>,
    by_site: &IndexMap<(String, String), String>,
    diagnostics: &mut DiagnosticBag,
    file_contents: &std::collections::HashMap<String, String>,
) -> Result<(), Error> {
    let file = f.file_path.as_ref().map(|p| p.to_string());
    f.body = rewrite_function_body(
        arena,
        f.body,
        by_site,
        diagnostics,
        file.as_deref(),
        file_contents,
    )?;
    Ok(())
}
