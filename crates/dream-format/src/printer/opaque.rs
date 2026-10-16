use super::*;

impl Printer {
    pub(super) fn print_opaque(
        &mut self,
        source: &str,
        start: usize,
        close: usize,
        end: usize,
        tokens: &[SyntaxToken],
    ) {
        let name = &tokens[start];
        self.set_anchor_offset(name);
        self.insert_blank_lines(name.kind);
        self.emit_leading_trivia(name);
        if self.layout.at_line_start() {
            self.layout.write_indent(self.current_indent(name.kind));
        } else if needs_space(
            self.prev_prev_kind,
            self.prev_kind.unwrap_or(TokenKind::EndOfFileToken),
            name.kind,
        ) {
            self.layout.space();
        }
        self.layout.text(&name.text);
        if let Some(payload) = source.get(name.position.end..end) {
            self.layout.verbatim(payload);
        }
        self.emit_trailing_trivia(&tokens[close]);
        self.prev_kind = Some(name.kind);
        self.update_prev_tracking(&tokens[close]);
    }
}
