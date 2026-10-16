//! Dream source formatter.
//!
//! Re-emits the lexer token stream (with attached comment trivia), rewriting only inter-token
//! whitespace, newlines, and indentation. Token text and comment contents are retained (comment indentation may change) —
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
mod groups;
mod layout;
mod line_index;
mod opaque;
mod printer;
mod spacing;
mod trivia;

#[cfg(test)]
mod tests;

pub(crate) const MAX_WIDTH: usize = 100;

pub(crate) const INDENT_UNIT: &str = "    ";

/// Pretty-prints `text`, retaining input unchanged when it cannot be safely formatted.
/// Successfully formatted output ends with exactly one trailing newline.
pub fn format(text: &str) -> String {
    try_format(text).unwrap_or_else(|| text.to_string())
}

/// Like [`format`], but returns `None` when the input cannot be safely reformatted.
pub fn try_format(text: &str) -> Option<String> {
    let tokens = lex(text)?;
    let formatted = printer::Printer::new(text).run(text, &tokens);
    let rewritten = lex(&formatted)?;
    if content(&tokens) != content(&rewritten) {
        return None;
    }
    Some(formatted)
}

fn content(tokens: &[SyntaxToken]) -> Vec<(dream_syntax::token::token_kind::TokenKind, String)> {
    let mut result = Vec::new();
    for token in tokens {
        for trivia in &token.leading_trivia {
            result.push((trivia.kind, normalize_comment(&trivia.text)));
        }
        result.push((token.kind, token.text.clone()));
        for trivia in &token.trailing_trivia {
            result.push((trivia.kind, normalize_comment(&trivia.text)));
        }
    }
    result
}

fn normalize_comment(text: &str) -> String {
    text.lines().map(str::trim).collect::<Vec<_>>().join("\n")
}

/// Lexes `text`, or `None` when any token fails to lex (the formatter must never drop bytes).
fn lex(text: &str) -> Option<Vec<SyntaxToken>> {
    let mut diagnostics = DiagnosticBag::new(None);
    let mut lexer = Lexer::new(text.to_string());
    let mut tokens = lexer.lex_all(&mut diagnostics);
    if diagnostics.has_errors() {
        return None;
    }
    attach_comments(text, &mut tokens);
    Some(tokens)
}

fn attach_comments(text: &str, tokens: &mut [SyntaxToken]) {
    let lines = line_index::LineIndex::new(text);
    let mut comments = Vec::new();
    for token in tokens.iter_mut() {
        comments.append(&mut token.leading_trivia);
        comments.append(&mut token.trailing_trivia);
    }
    comments.sort_by_key(|comment| comment.position.start);
    let mut next = 0usize;
    for comment in comments {
        while next + 1 < tokens.len() && tokens[next].position.start < comment.position.start {
            next += 1;
        }
        // Lexer positions are bytes; source line decisions must use byte-indexed lines too,
        // including when preceding comments contain non-ASCII characters.
        if next > 0
            && lines.line_of(comment.position.start)
                == lines.line_of(tokens[next - 1].position.end.saturating_sub(1))
        {
            tokens[next - 1].trailing_trivia.push(comment);
        } else {
            tokens[next].leading_trivia.push(comment);
        }
    }
}
