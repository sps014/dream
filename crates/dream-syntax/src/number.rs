//! Shared numeric-literal parsing. Lexer, classifier, HIR, and pattern ranges all go through
//! these helpers so `0xFF` / `0b101` / `1e-3` never silently fail a `str::parse`.

fn parse_u64_magnitude(s: &str) -> Option<u64> {
    if let Some(hex) = strip_prefix_ci(s, "0x") {
        u64::from_str_radix(hex, 16).ok()
    } else if let Some(bin) = strip_bin_body(s) {
        u64::from_str_radix(bin, 2).ok()
    } else if let Some(oct) = strip_prefix_ci(s, "0o") {
        u64::from_str_radix(oct, 8).ok()
    } else {
        s.parse::<u64>().ok()
    }
}

/// Signed integer value of a suffix-stripped numeric token (`42`, `0xFF`, `-0b101`, `0o77`).
/// Values outside `i64` (including unsigned magnitudes above `i64::MAX`) yield `None`.
pub fn parse_int_literal(text: &str) -> Option<i64> {
    let t = text.trim();
    let (neg, s) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t),
    };
    let u = parse_u64_magnitude(s)?;
    if neg {
        if u == (i64::MAX as u64) + 1 {
            Some(i64::MIN)
        } else if u <= i64::MAX as u64 {
            Some(-(u as i64))
        } else {
            None
        }
    } else if u <= i64::MAX as u64 {
        Some(u as i64)
    } else {
        None
    }
}

/// Unsigned integer value of a suffix-stripped token. Rejects a leading `-`.
pub fn parse_u64_literal(text: &str) -> Option<u64> {
    let t = text.trim();
    if t.starts_with('-') {
        return None;
    }
    parse_u64_magnitude(t)
}

/// Unsigned 32-bit value of a numeric token. Rejects negatives and values above `u32::MAX`.
pub fn parse_u32_literal(text: &str) -> Option<u32> {
    let v = parse_int_literal(text)?;
    if v < 0 || v > i64::from(u32::MAX) {
        None
    } else {
        Some(v as u32)
    }
}

/// Float value of a suffix-stripped token (`3.14`, `1e-3`, `-2.5e10`).
pub fn parse_float_literal(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok()
}

/// Body (prefix + digits / decimal / exponent) and type suffix of a number token.
/// `suffix` is lowercased (`"ul"`, `"f"`, …) or empty.
pub fn split_numeric_literal(text: &str) -> Option<(&str, &str)> {
    if text.is_empty() {
        return None;
    }
    let (neg, rest) = match text.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, text),
    };
    let (body_end, suffix) = split_unsigned(rest)?;
    let body = if neg {
        // Include the leading `-` in the body slice of `text`.
        &text[..1 + body_end]
    } else {
        &text[..body_end]
    };
    Some((body, suffix))
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// `0b`/`0B` followed by at least one binary digit — not the byte-suffix spelling `0b`.
fn strip_bin_body(s: &str) -> Option<&str> {
    let rest = strip_prefix_ci(s, "0b")?;
    if rest.is_empty() || !rest.bytes().all(|c| c == b'0' || c == b'1') {
        return None;
    }
    Some(rest)
}

fn split_unsigned(text: &str) -> Option<(usize, &str)> {
    let bytes = text.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_digit() {
        return None;
    }

    if bytes[0] == b'0' && bytes.len() >= 2 {
        match bytes[1] {
            b'x' | b'X' => return split_radix(text, 2, is_hex_digit, false),
            b'o' | b'O' => return split_radix(text, 2, is_oct_digit, true),
            b'b' | b'B' if bytes.len() > 2 && (bytes[2] == b'0' || bytes[2] == b'1') => {
                return split_radix(text, 2, is_bin_digit, true);
            }
            _ => {}
        }
    }

    split_decimal(text)
}

fn split_radix(
    text: &str,
    prefix_len: usize,
    is_digit: fn(u8) -> bool,
    allow_byte_suffix: bool,
) -> Option<(usize, &str)> {
    let bytes = text.as_bytes();
    let mut i = prefix_len;
    while i < bytes.len() && is_digit(bytes[i]) {
        i += 1;
    }
    if i == prefix_len {
        return None;
    }
    let suffix = suffix_from(&text[i..], allow_byte_suffix, false)?;
    Some((i, suffix))
}

fn split_decimal(text: &str) -> Option<(usize, &str)> {
    let bytes = text.as_bytes();
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
    let suffix = suffix_from(&text[i..], true, is_float)?;
    Some((i, suffix))
}

fn suffix_from(rest: &str, allow_byte: bool, is_float: bool) -> Option<&str> {
    if rest.is_empty() {
        return Some("");
    }
    let lower = rest.to_ascii_lowercase();
    let ok = if is_float {
        matches!(lower.as_str(), "f" | "d")
    } else if allow_byte {
        matches!(lower.as_str(), "b" | "u" | "l" | "ul" | "lu" | "f" | "d")
    } else {
        // Hex: `b`/`d`/`f` are digits, so only unsigned/long suffixes.
        matches!(lower.as_str(), "u" | "l" | "ul" | "lu")
    };
    if ok {
        Some(rest)
    } else {
        None
    }
}

fn is_hex_digit(c: u8) -> bool {
    c.is_ascii_hexdigit()
}
fn is_bin_digit(c: u8) -> bool {
    c == b'0' || c == b'1'
}
fn is_oct_digit(c: u8) -> bool {
    (b'0'..=b'7').contains(&c)
}

/// True when the (suffix-stripped) body is a floating literal.
pub fn numeric_body_is_float(body: &str) -> bool {
    let s = body.strip_prefix('-').unwrap_or(body);
    if s.len() >= 2 && s.as_bytes()[0] == b'0' {
        match s.as_bytes()[1] {
            b'x' | b'X' | b'b' | b'B' | b'o' | b'O' => return false,
            _ => {}
        }
    }
    s.contains('.') || s.contains('e') || s.contains('E')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prefixes() {
        assert_eq!(parse_int_literal("0xFF"), Some(255));
        assert_eq!(parse_int_literal("0x10"), Some(16));
        assert_eq!(parse_int_literal("-0x10"), Some(-16));
        assert_eq!(parse_int_literal("0b101"), Some(5));
        assert_eq!(parse_int_literal("0o77"), Some(63));
        assert_eq!(parse_int_literal("42"), Some(42));
        assert_eq!(parse_int_literal("9223372036854775807"), Some(i64::MAX));
        assert_eq!(parse_int_literal("9223372036854775808"), None);
        assert_eq!(parse_int_literal("-9223372036854775808"), Some(i64::MIN));
        assert_eq!(parse_u32_literal("0x40"), Some(64));
        assert_eq!(parse_u32_literal("0b101"), Some(5));
        assert_eq!(parse_u32_literal("-1"), None);
        assert_eq!(parse_u64_literal("18446744073709551615"), Some(u64::MAX));
        assert_eq!(parse_u64_literal("-1"), None);
        assert_eq!(parse_u64_literal("18446744073709551616"), None);
    }

    #[test]
    fn parses_scientific() {
        assert_eq!(parse_float_literal("1e-3"), Some(0.001));
        assert_eq!(parse_float_literal("1.5e2"), Some(150.0));
    }

    #[test]
    fn splits_suffixes() {
        assert_eq!(split_numeric_literal("42L"), Some(("42", "L")));
        assert_eq!(split_numeric_literal("0xFFu"), Some(("0xFF", "u")));
        assert_eq!(split_numeric_literal("0b101b"), Some(("0b101", "b")));
        assert_eq!(split_numeric_literal("0b"), Some(("0", "b")));
        assert_eq!(split_numeric_literal("1e-3"), Some(("1e-3", "")));
        assert_eq!(split_numeric_literal("1.2e10d"), Some(("1.2e10", "d")));
        assert_eq!(split_numeric_literal("255b"), Some(("255", "b")));
    }
}
