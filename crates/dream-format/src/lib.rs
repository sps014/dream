//! Dream source formatter.
//!
//! Re-emits the lexer token stream (with attached comment trivia), rewriting only inter-token
//! whitespace, newlines, and indentation. Token text and comments are preserved verbatim —
//! Dream's AST drops punctuation and trailing comments on `;`/`}`, so a full AST round-trip
//! cannot keep them faithfully.
//!
//! Formatting is lossless by construction: if the document contains anything the lexer rejects
//! (`DiagnosticBag` reports an error), the input is returned unchanged rather than formatted,
//! so a broken file can never lose characters. Generic type-argument regions (`Map<string, int>`)
//! are detected with the same balanced `<...>` scan the parser uses, so `<`/`>` inside them are
//! not spaced as comparison operators.

use dream_diagnostics::DiagnosticBag;
use dream_syntax::lexer::Lexer;
use dream_syntax::token::syntax_token::SyntaxToken;

mod generics;
mod layout;
mod line_index;
mod printer;
mod spacing;
mod trivia;

#[cfg(test)]
mod tests;

pub(crate) const INDENT_UNIT: &str = "    ";

/// Pretty-prints `text`. On input the lexer rejects, returns `text` unchanged (lossless).
/// Always ends with exactly one trailing newline.
pub fn format(text: &str) -> String {
    try_format(text).unwrap_or_else(|| text.to_string())
}

/// Like [`format`], but returns `None` when the input cannot be safely reformatted.
pub fn try_format(text: &str) -> Option<String> {
    let tokens = lex(text)?;
    Some(printer::Printer::new(text).run(&tokens))
}

/// Lexes `text`, or `None` when any token fails to lex (the formatter must never drop bytes).
fn lex(text: &str) -> Option<Vec<SyntaxToken>> {
    let mut diagnostics = DiagnosticBag::new(None);
    let mut lexer = Lexer::new(text.to_string());
    let tokens = lexer.lex_all(&mut diagnostics);
    if diagnostics.has_errors() {
        return None;
    }
    Some(tokens)
}
