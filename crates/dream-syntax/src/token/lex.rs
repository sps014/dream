//! Logos callbacks for numbers and interpolated strings. Kept off `token_kind.rs` so the
//! token enum stays a pure spelling table.

use super::token_kind::TokenKind;
use logos::Lexer;

/// Continues a number after logos matched the first digit. Returns `Err` so the span becomes a
/// `BadToken` for junk like `0xG` / `0b2` / `1e`.
///
/// `Result<(), ()>` is the logos callback contract for a unit token that can fail.
#[allow(clippy::result_unit_err)]
pub fn lex_number(lex: &mut Lexer<TokenKind>) -> Result<(), ()> {
    let rest = lex.remainder();
    let first = lex.slice().as_bytes()[0];
    let consumed = match scan_number_rest(first, rest) {
        Some(n) => n,
        None => {
            // Eat trailing alphanumerics so `0xG` is one unexpected token, not `0` + `xG`.
            let junk = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '.')
                .unwrap_or(rest.len());
            lex.bump(junk);
            return Err(());
        }
    };
    lex.bump(consumed);
    Ok(())
}

/// Bytes of `rest` that belong to the number that started with `first`.
fn scan_number_rest(first: u8, rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if first == b'0' && !bytes.is_empty() {
        match bytes[0] {
            b'x' | b'X' => return scan_radix(bytes, 1, is_hex, false),
            b'o' | b'O' => return scan_radix(bytes, 1, is_oct, true),
            b'b' | b'B' if bytes.len() > 1 && (bytes[1] == b'0' || bytes[1] == b'1') => {
                return scan_radix(bytes, 1, is_bin, true);
            }
            _ => {}
        }
    }
    scan_decimal(bytes)
}

fn scan_radix(
    bytes: &[u8],
    prefix_len: usize,
    is_digit: fn(u8) -> bool,
    allow_byte_suffix: bool,
) -> Option<usize> {
    let mut i = prefix_len;
    while i < bytes.len() && is_digit(bytes[i]) {
        i += 1;
    }
    if i == prefix_len {
        return None;
    }
    let after = consume_suffix(&bytes[i..], allow_byte_suffix, false)?;
    Some(i + after)
}

fn scan_decimal(bytes: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let mut is_float = false;
    if i < bytes.len() && bytes[i] == b'.' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
        is_float = true;
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j == exp_start {
            return None;
        }
        is_float = true;
        i = j;
    }
    let after = consume_suffix(&bytes[i..], true, is_float)?;
    Some(i + after)
}

fn consume_suffix(bytes: &[u8], allow_byte: bool, is_float: bool) -> Option<usize> {
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return Some(0);
    }
    // Longest suffix first.
    let candidates: &[&[u8]] = if is_float {
        &[b"d", b"D", b"f", b"F"]
    } else if allow_byte {
        &[
            b"uL", b"ul", b"Ul", b"UL", b"Lu", b"lU", b"LU", b"lu", b"b", b"B", b"u", b"U", b"l",
            b"L", b"d", b"D", b"f", b"F",
        ]
    } else {
        &[
            b"uL", b"ul", b"Ul", b"UL", b"Lu", b"lU", b"LU", b"lu", b"u", b"U", b"l", b"L",
        ]
    };
    for suf in candidates {
        if bytes.len() >= suf.len() && bytes[..suf.len()].eq_ignore_ascii_case(suf) {
            let rest = &bytes[suf.len()..];
            if rest.first().is_some_and(|c| c.is_ascii_alphanumeric()) {
                return None;
            }
            return Some(suf.len());
        }
    }
    None
}

fn is_hex(c: u8) -> bool {
    c.is_ascii_hexdigit()
}
fn is_bin(c: u8) -> bool {
    c == b'0' || c == b'1'
}
fn is_oct(c: u8) -> bool {
    (b'0'..=b'7').contains(&c)
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
