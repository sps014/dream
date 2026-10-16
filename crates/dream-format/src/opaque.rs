use std::collections::BTreeMap;

use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::{lexer::Lexer, parser::Parser};

/// Syntax-block payloads have their own grammar, including meaningful token adjacency.
/// Parser ranges keep them out of Dream's whitespace rules without guessing DSL names.
pub(crate) fn regions(source: &str) -> Vec<(usize, usize)> {
    let arena = Bump::new();
    let mut diagnostics = DiagnosticBag::new(None);
    let mut parser = Parser::new(Lexer::new(source.to_owned()), &arena, &mut diagnostics);
    let Ok(tree) = parser.parse() else {
        return Vec::new();
    };
    let mut spans = BTreeMap::<usize, usize>::new();
    for span in &tree.get_root().node_spans {
        if span.expression
            && span.kind == "SyntaxBlock"
            && let Some(start) = span.anchor
            && start < span.span.end
            && span.span.end <= source.len()
        {
            spans
                .entry(start)
                .and_modify(|end| *end = (*end).min(span.span.end))
                .or_insert(span.span.end);
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
