use super::super::Parser;
use crate::nodes::ExpressionNode;
use crate::token::syntax_token::SyntaxToken;
use std::io::Error;

impl<'a, 'b> Parser<'a, 'b> {
    /// Lowers a raw string literal `"""..."""` to an ordinary string literal. The body has no
    /// escapes, so `\` and `"` are escaped for the literal node and nothing past the parser sees
    /// a raw string.
    pub(crate) fn parse_raw_string(
        &mut self,
        token: SyntaxToken,
    ) -> Result<ExpressionNode<'a>, Error> {
        let pos = token.position;
        let chars = self.raw_string_chars(&token, 0);
        let mut text = String::with_capacity(chars.len());
        for (c, _) in chars {
            if matches!(c, '\\' | '"') {
                text.push('\\');
            }
            text.push(c);
        }
        Ok(self.make_string_literal(text, pos))
    }

    /// Lowers `$"""...{expr}..."""` through the same `+` chain as `$"..."`.
    pub(crate) fn parse_interpolated_raw_string(
        &mut self,
        token: SyntaxToken,
    ) -> Result<ExpressionNode<'a>, Error> {
        let pos = token.position;
        let chars = self.raw_string_chars(&token, 1);
        self.lower_interpolation(&token.text, &chars, pos, true)
    }

    /// The characters a raw literal denotes, each with its absolute file offset. `prefix` is the
    /// length of the `$` before the opening quotes. Errors are reported and yield an empty body.
    fn raw_string_chars(&mut self, token: &SyntaxToken, prefix: usize) -> Vec<(char, usize)> {
        let pos = token.position;
        let text = &token.text[prefix..];
        let fence = text.bytes().take_while(|&b| b == b'"').count();
        let closing = text.bytes().rev().take_while(|&b| b == b'"').count();
        if text.len() < fence * 2 || closing < fence {
            self.diagnostics.report_error(
                format!(
                    "unterminated raw string literal; close it with {}",
                    "\"".repeat(fence)
                ),
                Some(pos),
            );
            return Vec::new();
        }
        if closing > fence {
            self.diagnostics.report_error(
                format!(
                    "raw string literal closes with more quotes than it opens with; open it with {}",
                    "\"".repeat(closing)
                ),
                Some(pos),
            );
            return Vec::new();
        }

        let body_base = pos.start + prefix + fence;
        let body = &text[fence..text.len() - fence];
        let mut chars: Vec<(char, usize)> = Vec::with_capacity(body.len());
        let mut iter = body.char_indices().peekable();
        while let Some((offset, c)) = iter.next() {
            if c == '\r' && iter.peek().is_some_and(|&(_, next)| next == '\n') {
                continue;
            }
            chars.push((c, body_base + offset));
        }
        if !chars.iter().any(|&(c, _)| c == '\n') {
            return chars;
        }

        let lines: Vec<&[(char, usize)]> = chars.split(|&(c, _)| c == '\n').collect();
        let is_blank = |line: &[(char, usize)]| line.iter().all(|&(c, _)| c == ' ' || c == '\t');
        let (Some(first), Some(last)) = (lines.first(), lines.last()) else {
            return chars;
        };
        if !is_blank(first) {
            self.diagnostics.report_error(
                "a multi-line raw string must start on the line after its opening quotes"
                    .to_string(),
                Some(pos),
            );
            return Vec::new();
        }
        if !is_blank(last) {
            self.diagnostics.report_error(
                "the closing quotes of a multi-line raw string must be on their own line"
                    .to_string(),
                Some(pos),
            );
            return Vec::new();
        }

        let indent: Vec<char> = last.iter().map(|&(c, _)| c).collect();
        let content = &lines[1..lines.len() - 1];
        let mut out = Vec::with_capacity(chars.len());
        for (index, line) in content.iter().enumerate() {
            if index > 0 {
                let newline = line.first().map_or(pos.start, |&(_, offset)| offset - 1);
                out.push(('\n', newline));
            }
            if is_blank(line) {
                continue;
            }
            let starts_with_indent = line.len() >= indent.len()
                && line.iter().zip(&indent).all(|(&(c, _), &want)| c == want);
            if !starts_with_indent {
                self.diagnostics.report_error(
                    "a raw string line must start with the same whitespace as its closing quotes"
                        .to_string(),
                    Some(pos),
                );
                return Vec::new();
            }
            out.extend_from_slice(&line[indent.len()..]);
        }
        out
    }
}
