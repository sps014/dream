use super::Parser;
use crate::nodes::program::SourceNodeSpan;
use dream_text::text_span::TextSpan;

impl Parser<'_, '_> {
    /// Retains complete ranges when an AST representative token omits source punctuation.
    pub(super) fn record_node_span(
        &mut self,
        expression: bool,
        kind: &'static str,
        anchor: Option<usize>,
        mut span: TextSpan,
    ) {
        if let Some(last) = self.tokens.get(self.current_token_index.saturating_sub(1)) {
            span.end = span.end.max(last.position.end);
        }
        self.node_spans
            .entry((expression, kind, anchor, span.start))
            .and_modify(|existing| existing.span.end = existing.span.end.max(span.end))
            .or_insert(SourceNodeSpan {
                expression,
                kind,
                anchor,
                span,
            });
    }
}
