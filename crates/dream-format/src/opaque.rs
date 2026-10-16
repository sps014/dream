use std::collections::BTreeMap;

use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::{lexer::Lexer, parser::Parser};

/// Syntax-block payloads have their own grammar, including meaningful token adjacency.
/// Parser ranges keep them out of Dream's whitespace rules without guessing DSL names.
pub(crate) fn regions(source: &str) -> Vec<(usize, usize)> {
    let mut spans = BTreeMap::<usize, usize>::new();
    let complete = collect(source, 0, source.len(), &mut spans);
    if !complete {
        // Documentation snippets are often statement fragments; parsing them as a function
        // body locates their payloads. Recovery ranges from a failed parse are not trusted.
        let prefix = "fun __fragment() {\n";
        let mut fragment = BTreeMap::new();
        if collect(
            &format!("{prefix}{source}\n}}"),
            prefix.len(),
            source.len(),
            &mut fragment,
        ) {
            spans.extend(fragment);
        }
    }
    let mut result = Vec::new();
    let mut covered = 0;
    for (start, end) in spans {
        if start >= covered {
            result.push((start, end));
            covered = end;
        }
    }
    result
}

/// Records syntax-block ranges of `text`, shifted back by `offset` and limited to `len`
/// bytes of the original source. Returns whether `text` parsed without errors.
fn collect(text: &str, offset: usize, len: usize, spans: &mut BTreeMap<usize, usize>) -> bool {
    let arena = Bump::new();
    let mut diagnostics = DiagnosticBag::new(None);
    let mut parser = Parser::new(Lexer::new(text.to_owned()), &arena, &mut diagnostics);
    let Ok(tree) = parser.parse() else {
        return false;
    };
    for span in &tree.get_root().node_spans {
        if span.expression
            && span.kind == "SyntaxBlock"
            && let Some(anchor) = span.anchor
            && anchor >= offset
            && anchor < span.span.end
            && span.span.end - offset <= len
        {
            let (start, end) = (anchor - offset, span.span.end - offset);
            spans
                .entry(start)
                .and_modify(|current| *current = (*current).min(end))
                .or_insert(end);
        }
    }
    !diagnostics.has_errors()
}
