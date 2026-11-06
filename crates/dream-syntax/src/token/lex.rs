//! Logos callbacks for numbers and interpolated strings. Kept off `token_kind.rs` so the
//! token enum stays a pure spelling table.

use super::token_kind::TokenKind;
use logos::Lexer;

/// Continues a number after logos matched the first digit. Returns `Err` so the span becomes a
/// `BadToken` for junk like `0xG` / `0b2` / `1e` / `1_`.
///
/// `Result<(), ()>` is the logos callback contract for a unit token that can fail.
#[allow(clippy::result_unit_err)]
pub fn lex_number(lex: &mut Lexer<TokenKind>) -> Result<(), ()> {
    let start = lex.span().start;
    let matched = lex.slice().len();
    let text = &lex.source()[start..];
    match crate::number::scan_numeric_literal(text) {
        Some((body, suffix)) => {
            lex.bump(body + suffix - matched);
            Ok(())
        }
        None => {
            // Eat trailing alphanumerics so `0xG` is one unexpected token, not `0` + `xG`.
            let rest = lex.remainder();
            let junk = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '_')
                .unwrap_or(rest.len());
            lex.bump(junk);
            Err(())
        }
    }
}

/// `"""` already matched. Extra opening quotes widen the fence, and the token ends at the first
/// run of at least that many quotes. An unterminated literal runs to end of input so the parser
/// can report it once.
///
/// `Result<(), ()>` is the logos callback contract for a unit token that can fail.
#[allow(clippy::result_unit_err)]
pub fn lex_raw_string(lex: &mut Lexer<TokenKind>) -> Result<(), ()> {
    lex_raw(lex, false);
    Ok(())
}

/// `$"""` already matched. Like [`lex_raw_string`], but a fence inside a `{...}` hole does not
/// close the literal.
///
/// `Result<(), ()>` is the logos callback contract for a unit token that can fail.
#[allow(clippy::result_unit_err)]
pub fn lex_interpolated_raw_string(lex: &mut Lexer<TokenKind>) -> Result<(), ()> {
    lex_raw(lex, true);
    Ok(())
}

fn lex_raw(lex: &mut Lexer<TokenKind>, interpolated: bool) {
    let rest = lex.remainder().as_bytes();
    let extra = rest.iter().take_while(|&&b| b == b'"').count();
    let fence = 3 + extra;
    let mut i = extra;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    while i < rest.len() {
        let c = rest[i];
        if depth > 0 {
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == b'\\' {
                    escaped = true;
                } else if c == b'"' {
                    in_string = false;
                }
            } else {
                match c {
                    b'"' => in_string = true,
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    _ => {}
                }
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            let run = rest[i..].iter().take_while(|&&b| b == b'"').count();
            if run >= fence {
                lex.bump(i + run);
                return;
            }
            i += run;
            continue;
        }
        if interpolated && c == b'{' {
            if rest.get(i + 1) == Some(&b'{') {
                i += 2;
                continue;
            }
            depth = 1;
        }
        i += 1;
    }
    lex.bump(rest.len());
}

/// `$"` already matched. Scan until the closing `"` at interpolation depth 0, skipping quotes
/// that sit inside `{...}` holes (so `$"x is {"hi"}"` is one token).
///
/// `Result<(), ()>` is the logos callback contract for a unit token that can fail.
#[allow(clippy::result_unit_err)]
pub fn lex_interpolated_string(lex: &mut Lexer<TokenKind>) -> Result<(), ()> {
    let rest = lex.remainder();
    let bytes = rest.as_bytes();
    let mut i = 0;
    let mut depth = 0;
    let mut in_string = false;
    let mut in_char = false;
    let mut escaped = false;
    while i < bytes.len() {
        let c = bytes[i];
        if depth == 0 {
            if escaped {
                escaped = false;
                i += 1;
                continue;
            }
            match c {
                b'\\' => {
                    escaped = true;
                    i += 1;
                }
                b'"' => {
                    lex.bump(i + 1);
                    return Ok(());
                }
                b'{' => {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                        i += 2;
                    } else {
                        depth = 1;
                        i += 1;
                    }
                }
                b'}' => {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'}' {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                _ => i += 1,
            }
            continue;
        }
        // Inside a hole: ignore braces/quotes that sit in string or char literals.
        if escaped {
            escaped = false;
            i += 1;
            continue;
        }
        if in_string {
            if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if in_char {
            if c == b'\\' {
                escaped = true;
            } else if c == b'\'' {
                in_char = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'\\' => {
                escaped = true;
                i += 1;
            }
            b'"' => {
                in_string = true;
                i += 1;
            }
            b'\'' => {
                in_char = true;
                i += 1;
            }
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    Err(())
}
