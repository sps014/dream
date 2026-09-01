//! Computes LSP semantic tokens by lexing the document and classifying each identifier against
//! the symbol [`crate::index::Index`] (so a name colours as the function/struct/field/etc. it
//! actually refers to), then delta-encoding the result as the protocol requires.

use dream::diagnostics::DiagnosticBag;
use dream::syntax::lexer::Lexer;
use dream::syntax::token::syntax_token::SyntaxToken;
use dream::syntax::token::syntax_trivia::SyntaxTrivia;
use dream::syntax::token::token_kind::{TokenKind, SOFT_SPECIALS};
use tower_lsp::lsp_types::{SemanticToken, SemanticTokenType};

use crate::index::{Index, SymKind};
use crate::position::LineIndex;
use crate::tokens::{lex_category, LexCategory};

/// The ordered semantic-token legend advertised in the server capabilities. A token's
/// `token_type` is an index into this slice.
pub const TOKEN_TYPES: [SemanticTokenType; 17] = [
    SemanticTokenType::KEYWORD,     // 0
    SemanticTokenType::VARIABLE,    // 1
    SemanticTokenType::PROPERTY,    // 2
    SemanticTokenType::FUNCTION,    // 3
    SemanticTokenType::METHOD,      // 4
    SemanticTokenType::CLASS,       // 5
    SemanticTokenType::ENUM,        // 6
    SemanticTokenType::ENUM_MEMBER, // 7
    SemanticTokenType::PARAMETER,   // 8
    SemanticTokenType::TYPE,        // 9
    SemanticTokenType::OPERATOR,    // 10
    SemanticTokenType::STRING,      // 11
    SemanticTokenType::NUMBER,      // 12
    SemanticTokenType::COMMENT,     // 13
    SemanticTokenType::DECORATOR,   // 14 — `@json`, `@get_indexer`, …
    SemanticTokenType::STRUCT,      // 15
    SemanticTokenType::INTERFACE,   // 16
];

const KEYWORD: u32 = 0;
const COMMENT: u32 = 13;
const DECORATOR: u32 = 14;
const OPERATOR: u32 = 10;
const STRING: u32 = 11;
const STRUCT: u32 = 15;
const INTERFACE: u32 = 16;

/// Index of a symbol kind into [`TOKEN_TYPES`].
fn sym_kind_token_index(kind: SymKind) -> u32 {
    match kind {
        SymKind::Function => 3,
        SymKind::Class => 5,
        SymKind::Struct => STRUCT,
        SymKind::Interface => INTERFACE,
        SymKind::Enum => 6,
        SymKind::EnumMember => 7,
        SymKind::Field => 2,
        SymKind::Method => 4,
        SymKind::Variable => 1,
        SymKind::Param => 8,
        SymKind::Type => 9,
        SymKind::Keyword => KEYWORD,
        SymKind::Decorator => DECORATOR,
        SymKind::Module => 9, // reuse type slot; modules are rare in semantic tokens
    }
}

fn push_trivia_comments(
    trivia: &[SyntaxTrivia],
    line_index: &LineIndex,
    out: &mut Vec<(u32, u32, u32, u32)>,
) {
    for t in trivia {
        if !matches!(
            t.kind,
            TokenKind::LineCommentToken | TokenKind::BlockCommentToken
        ) {
            continue;
        }
        // Multi-line block comments: emit one token per line so delta-encoding stays valid.
        let mut offset = t.position.start;
        for (i, line) in t.text.split('\n').enumerate() {
            if line.is_empty() && i + 1 == t.text.split('\n').count() {
                break;
            }
            let len = line.chars().count() as u32;
            if len == 0 {
                offset += 1; // the newline
                continue;
            }
            let start_pos = line_index.position(offset);
            out.push((start_pos.line, start_pos.character, len, COMMENT));
            offset += line.len() + 1;
        }
    }
}

fn push_raw_span(
    line_index: &LineIndex,
    out: &mut Vec<(u32, u32, u32, u32)>,
    token_start: usize,
    raw: &str,
    rel: usize,
    len: usize,
    type_idx: u32,
) {
    if len == 0 || rel + len > raw.len() {
        return;
    }
    let slice = &raw[rel..rel + len];
    if slice.contains('\n') {
        return;
    }
    let start_pos = line_index.position(token_start + rel);
    out.push((
        start_pos.line,
        start_pos.character,
        slice.chars().count() as u32,
        type_idx,
    ));
}

fn push_text_span(
    line_index: &LineIndex,
    out: &mut Vec<(u32, u32, u32, u32)>,
    text: &str,
    start: usize,
    len: usize,
    type_idx: u32,
) {
    if len == 0 || start + len > text.len() {
        return;
    }
    let slice = &text[start..start + len];
    if slice.contains('\n') {
        return;
    }
    let start_pos = line_index.position(start);
    out.push((
        start_pos.line,
        start_pos.character,
        slice.chars().count() as u32,
        type_idx,
    ));
}

/// Emit STRING spans for literal parts of `$"…{expr}…"`, OPERATOR for `{`/`}`, and leave hole
/// interiors uncovered so TextMate can color expressions inside.
fn push_interpolated_string(
    line_index: &LineIndex,
    out: &mut Vec<(u32, u32, u32, u32)>,
    token_start: usize,
    raw: &str,
) {
    let Some(body) = raw.strip_prefix("$\"").and_then(|s| s.strip_suffix('"')) else {
        push_raw_span(line_index, out, token_start, raw, 0, raw.len(), STRING);
        return;
    };

    push_raw_span(line_index, out, token_start, raw, 0, 2, STRING); // `$"`

    let chars: Vec<char> = body.chars().collect();
    let mut i = 0usize;
    let mut byte_pos = 0usize;
    let mut lit_rel_start = 2usize; // offset into `raw` after `$"`

    while i < chars.len() {
        let c = chars[i];
        if c == '{' {
            if i + 1 < chars.len() && chars[i + 1] == '{' {
                byte_pos += 2;
                i += 2;
                continue;
            }
            let brace_rel = 2 + byte_pos;
            if brace_rel > lit_rel_start {
                push_raw_span(
                    line_index,
                    out,
                    token_start,
                    raw,
                    lit_rel_start,
                    brace_rel - lit_rel_start,
                    STRING,
                );
            }
            push_raw_span(line_index, out, token_start, raw, brace_rel, 1, OPERATOR);
            byte_pos += 1;
            i += 1;
            let mut depth = 1i32;
            while i < chars.len() && depth > 0 {
                let hc = chars[i];
                if hc == '{' {
                    if i + 1 < chars.len() && chars[i + 1] == '{' {
                        byte_pos += 2;
                        i += 2;
                        continue;
                    }
                    depth += 1;
                    byte_pos += hc.len_utf8();
                    i += 1;
                } else if hc == '}' {
                    depth -= 1;
                    if depth == 0 {
                        let close_rel = 2 + byte_pos;
                        push_raw_span(line_index, out, token_start, raw, close_rel, 1, OPERATOR);
                        byte_pos += 1;
                        i += 1;
                        lit_rel_start = 2 + byte_pos;
                        break;
                    }
                    byte_pos += hc.len_utf8();
                    i += 1;
                } else {
                    byte_pos += hc.len_utf8();
                    i += 1;
                }
            }
            if depth != 0 {
                lit_rel_start = 2 + byte_pos;
                break;
            }
            continue;
        }
        if c == '}' {
            if i + 1 < chars.len() && chars[i + 1] == '}' {
                byte_pos += 2;
                i += 2;
                continue;
            }
            byte_pos += 1;
            i += 1;
            continue;
        }
        byte_pos += c.len_utf8();
        i += 1;
    }

    let end_quote_rel = raw.len().saturating_sub(1);
    if end_quote_rel > lit_rel_start {
        push_raw_span(
            line_index,
            out,
            token_start,
            raw,
            lit_rel_start,
            end_quote_rel - lit_rel_start,
            STRING,
        );
    }
    if raw.ends_with('"') {
        push_raw_span(line_index, out, token_start, raw, end_quote_rel, 1, STRING);
    }
}

fn next_significant_kind(tokens: &[SyntaxToken], from: usize) -> Option<TokenKind> {
    tokens.get(from..)?.iter().find_map(|t| {
        if matches!(t.kind, TokenKind::EndOfFileToken | TokenKind::BadToken) {
            None
        } else {
            Some(t.kind)
        }
    })
}

pub fn compute(file_path: Option<&str>, text: &str) -> Vec<SemanticToken> {
    let mut scratch = DiagnosticBag::new(None);
    let mut lexer = Lexer::new(text.to_string());
    let tokens = lexer.lex_all(&mut scratch);
    let idx = Index::build(file_path, text);
    compute_from(&idx, tokens, text)
}

/// Like [`compute`], but reuses a caller-supplied [`Index`] (the per-document cached model) and
/// lexes only — no second parse/index walk.
pub fn compute_cached(idx: &Index, text: &str) -> Vec<SemanticToken> {
    let mut scratch = DiagnosticBag::new(None);
    let mut lexer = Lexer::new(text.to_string());
    let tokens = lexer.lex_all(&mut scratch);
    compute_from(idx, tokens, text)
}

fn compute_from(idx: &Index, tokens: Vec<SyntaxToken>, text: &str) -> Vec<SemanticToken> {
    let line_index = LineIndex::new(text);

    let mut semantic_tokens = Vec::new();
    let mut prev_was_at = false;

    for (ti, token) in tokens.iter().enumerate() {
        push_trivia_comments(&token.leading_trivia, &line_index, &mut semantic_tokens);

        if token.kind != TokenKind::EndOfFileToken && token.kind != TokenKind::BadToken {
            match token.kind {
                TokenKind::AtToken => {
                    prev_was_at = true;
                    push_text_span(
                        &line_index,
                        &mut semantic_tokens,
                        text,
                        token.position.start,
                        token.text.len(),
                        OPERATOR,
                    );
                }
                TokenKind::InterpolatedStringToken => {
                    prev_was_at = false;
                    push_interpolated_string(
                        &line_index,
                        &mut semantic_tokens,
                        token.position.start,
                        &token.text,
                    );
                }
                TokenKind::IdentifierToken => {
                    let soft_special = SOFT_SPECIALS.contains(&token.text.as_str())
                        && matches!(
                            next_significant_kind(&tokens, ti + 1),
                            Some(TokenKind::OpenParenthesisToken)
                        );
                    let kind = if prev_was_at {
                        prev_was_at = false;
                        DECORATOR
                    } else if token.text == "this" || soft_special {
                        KEYWORD
                    } else if let Some(decl) =
                        idx.decls.iter().find(|d| d.start == token.position.start)
                    {
                        sym_kind_token_index(decl.kind)
                    } else if let Some(r) =
                        idx.refs.iter().find(|r| r.start == token.position.start)
                    {
                        sym_kind_token_index(r.kind)
                    } else {
                        1 // variable
                    };
                    push_text_span(
                        &line_index,
                        &mut semantic_tokens,
                        text,
                        token.position.start,
                        token.text.len(),
                        kind,
                    );
                }
                other => {
                    prev_was_at = false;
                    if let Some(c) = lex_category(other) {
                        let type_idx = match c {
                            LexCategory::Keyword => KEYWORD,
                            LexCategory::Type => 9,
                            LexCategory::Operator => OPERATOR,
                            LexCategory::String => STRING,
                            LexCategory::Number => 12,
                        };
                        push_text_span(
                            &line_index,
                            &mut semantic_tokens,
                            text,
                            token.position.start,
                            token.text.len(),
                            type_idx,
                        );
                    }
                }
            }
        } else {
            prev_was_at = false;
        }

        push_trivia_comments(&token.trailing_trivia, &line_index, &mut semantic_tokens);
    }

    semantic_tokens.sort_by_key(|t| (t.0, t.1));

    let mut result = Vec::new();
    let mut pre_line = 0;
    let mut pre_char = 0;

    for (line, char, len, type_idx) in semantic_tokens {
        let delta_line = line - pre_line;
        let delta_start = if delta_line == 0 {
            char - pre_char
        } else {
            char
        };

        result.push(SemanticToken {
            delta_line,
            delta_start,
            length: len,
            token_type: type_idx,
            token_modifiers_bitset: 0,
        });
        pre_line = line;
        pre_char = char;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn absolute_spans(text: &str) -> Vec<(usize, usize, u32)> {
        let tokens = compute(None, text);
        let line_index = LineIndex::new(text);
        let mut line = 0u32;
        let mut character = 0u32;
        let mut out = Vec::new();
        for t in tokens {
            if t.delta_line > 0 {
                line += t.delta_line;
                character = t.delta_start;
            } else {
                character += t.delta_start;
            }
            // LSP deltas are relative to the *start* of the previous token (not its end).
            let abs_start = line_index.offset(line, character);
            let abs_end = line_index.offset(line, character + t.length);
            out.push((abs_start, abs_end - abs_start, t.token_type));
        }
        out
    }

    #[test]
    fn interpolated_string_splits_holes_from_string() {
        let src = r#"fun main(): void { let s = $"hi {name}"; }"#;
        let spans = absolute_spans(src);
        let interp_open = src.find("hi {").map(|i| i + 3).unwrap();
        let interp_close = src[interp_open..]
            .find('}')
            .map(|i| interp_open + i)
            .unwrap();

        let open_ty = spans
            .iter()
            .find(|(start, len, _)| *start == interp_open && *len == 1)
            .map(|(_, _, ty)| *ty);
        let close_ty = spans
            .iter()
            .find(|(start, len, _)| *start == interp_close && *len == 1)
            .map(|(_, _, ty)| *ty);
        assert_eq!(
            open_ty,
            Some(OPERATOR),
            "expected `{{` as OPERATOR, spans={spans:?}"
        );
        assert_eq!(
            close_ty,
            Some(OPERATOR),
            "expected `}}` as OPERATOR, spans={spans:?}"
        );

        let name_start = interp_open + 1;
        let name_covered_as_string = spans.iter().any(|(start, len, ty)| {
            *ty == STRING && *start <= name_start && name_start < *start + *len
        });
        assert!(
            !name_covered_as_string,
            "hole interior must not be STRING, spans={spans:?}"
        );
    }

    #[test]
    fn sizeof_followed_by_paren_is_keyword() {
        let src = "fun main(): void { let n = sizeof(int); }";
        let spans = absolute_spans(src);
        let start = src.find("sizeof").unwrap();
        let ty = spans
            .iter()
            .find(|(s, len, _)| *s == start && *len == "sizeof".len())
            .map(|(_, _, ty)| *ty);
        assert_eq!(ty, Some(KEYWORD), "spans={spans:?}");
    }
}
