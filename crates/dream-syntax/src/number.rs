//! Shared numeric-literal parsing. Lexer, classifier, HIR, and pattern ranges all go through
//! these helpers so `0xFF` / `0b101` / `1e-3` never silently fail a `str::parse`.

fn without_separators(s: &str) -> std::borrow::Cow<'_, str> {
    if s.contains('_') {
        std::borrow::Cow::Owned(s.replace('_', ""))
    } else {
        std::borrow::Cow::Borrowed(s)
    }
}

fn parse_u64_magnitude(s: &str) -> Option<u64> {
    let s = &*without_separators(s);
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

/// Whether a numeric token is spelled in hexadecimal, octal, or binary.
pub fn is_radix_literal(text: &str) -> bool {
    let s = text.trim();
    strip_prefix_ci(s, "0x").is_some()
        || strip_prefix_ci(s, "0o").is_some()
        || strip_bin_body(s).is_some()
}

/// Value of a suffix-stripped token as a `bits`-wide signed integer. A `0x`/`0o`/`0b` literal
/// spells a bit pattern, so any value up to the unsigned maximum of the width is accepted and read
/// as two's complement (`0xFFFFFFFF` is `-1` at 32 bits). A decimal literal must fit the signed range.
pub fn parse_signed_literal(text: &str, bits: u32) -> Option<i64> {
    debug_assert!((1..=64).contains(&bits));
    if is_radix_literal(text) {
        let u = parse_u64_magnitude(text.trim())?;
        let shift = 64 - bits;
        if bits < 64 && u >> bits != 0 {
            return None;
        }
        return Some(((u << shift) as i64) >> shift);
    }
    let v = parse_int_literal(text)?;
    let min = i64::MIN >> (64 - bits);
    let max = i64::MAX >> (64 - bits);
    (min..=max).contains(&v).then_some(v)
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
    without_separators(text.trim()).parse::<f64>().ok()
}

/// Body (prefix + digits / decimal / exponent) and type suffix of a whole number token.
/// `suffix` keeps its source spelling (`"UL"`, `"f"`, …) or is empty.
pub fn split_numeric_literal(text: &str) -> Option<(&str, &str)> {
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let sign = text.len() - unsigned.len();
    let (body, suffix) = scan_numeric_literal(unsigned)?;
    if body + suffix != unsigned.len() {
        return None;
    }
    Some((&text[..sign + body], &unsigned[body..]))
}

/// Byte lengths of the body and type suffix of the number at the start of `text`. `None` when
/// the text starts like a number but is malformed (`0xG`, `0b2`, `1e`, `1_`, `1z`). A `_` may
/// separate two digits of the same run (`1_000_000`, `0xFFFF_0000`).
pub fn scan_numeric_literal(text: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_digit) {
        return None;
    }
    if bytes[0] == b'0' {
        match bytes.get(1) {
            Some(b'x' | b'X') => return scan_radix(bytes, is_hex_digit, false),
            Some(b'o' | b'O') => return scan_radix(bytes, is_oct_digit, true),
            Some(b'b' | b'B') if bytes.get(2).is_some_and(|&c| is_bin_digit(c)) => {
                return scan_radix(bytes, is_bin_digit, true);
            }
            _ => {}
        }
    }
    scan_decimal(bytes)
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
    if rest.is_empty() || !rest.bytes().all(|c| is_bin_digit(c) || c == b'_') {
        return None;
    }
    Some(rest)
}

/// End of the digit run starting at `start`. A `_` that is not followed by a digit is malformed.
fn digit_run(bytes: &[u8], start: usize, is_digit: fn(u8) -> bool) -> Option<usize> {
    let mut i = start;
    while let Some(&c) = bytes.get(i) {
        if is_digit(c) {
            i += 1;
        } else if c == b'_' && i > start {
            if !bytes.get(i + 1).is_some_and(|&n| is_digit(n)) {
                return None;
            }
            i += 1;
        } else {
            break;
        }
    }
    (i > start).then_some(i)
}

fn scan_radix(
    bytes: &[u8],
    is_digit: fn(u8) -> bool,
    allow_byte_suffix: bool,
) -> Option<(usize, usize)> {
    let end = digit_run(bytes, 2, is_digit)?;
    let suffix = suffix_len(&bytes[end..], allow_byte_suffix, false)?;
    Some((end, suffix))
}

fn scan_decimal(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut i = digit_run(bytes, 0, |c| c.is_ascii_digit())?;
    let mut is_float = false;
    if bytes.get(i) == Some(&b'.') && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
        is_float = true;
        i = digit_run(bytes, i + 1, |c| c.is_ascii_digit())?;
    }
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(bytes.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        i = digit_run(bytes, j, |c| c.is_ascii_digit())?;
        is_float = true;
    }
    let suffix = suffix_len(&bytes[i..], true, is_float)?;
    Some((i, suffix))
}

/// Length of the type suffix at the start of `rest`. The whole alphanumeric run must be a suffix
/// allowed for the literal's kind, so `1z` and `0xFFq` are malformed rather than split.
fn suffix_len(rest: &[u8], allow_byte: bool, is_float: bool) -> Option<usize> {
    let len = rest
        .iter()
        .take_while(|c| c.is_ascii_alphanumeric())
        .count();
    if len == 0 {
        return Some(0);
    }
    let lower = rest[..len].to_ascii_lowercase();
    let ok = if is_float {
        matches!(lower.as_slice(), b"f" | b"d" | b"h")
    } else if allow_byte {
        matches!(
            lower.as_slice(),
            b"b" | b"u" | b"l" | b"ul" | b"lu" | b"f" | b"d" | b"h"
        )
    } else {
        // Hex: `b`/`d`/`f` are digits, so only unsigned/long suffixes.
        matches!(lower.as_slice(), b"u" | b"l" | b"ul" | b"lu")
    };
    ok.then_some(len)
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
    fn radix_literals_spell_bit_patterns() {
        assert_eq!(parse_signed_literal("0xFFFFFFFF", 32), Some(-1));
        assert_eq!(
            parse_signed_literal("0x80000000", 32),
            Some(i64::from(i32::MIN))
        );
        assert_eq!(parse_signed_literal("0x7ff00000", 32), Some(0x7ff0_0000));
        assert_eq!(parse_signed_literal("0x100000000", 32), None);
        assert_eq!(
            parse_signed_literal("0x8000000000000000", 64),
            Some(i64::MIN)
        );
        assert_eq!(parse_signed_literal("0b11111111", 8), Some(-1));
        assert_eq!(parse_signed_literal("4294967295", 32), None);
        assert_eq!(
            parse_signed_literal("-2147483648", 32),
            Some(i64::from(i32::MIN))
        );
        assert_eq!(parse_signed_literal("-0x10", 32), Some(-16));
    }

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
        assert_eq!(split_numeric_literal("1.5h"), Some(("1.5", "h")));
        assert_eq!(split_numeric_literal("2H"), Some(("2", "H")));
        assert_eq!(
            split_numeric_literal("1_000_000L"),
            Some(("1_000_000", "L"))
        );
        assert_eq!(
            split_numeric_literal("0xFFFF_0000u"),
            Some(("0xFFFF_0000", "u"))
        );
        assert_eq!(
            split_numeric_literal("1_000.5e1_0"),
            Some(("1_000.5e1_0", ""))
        );
        assert_eq!(split_numeric_literal("1_"), None);
        assert_eq!(split_numeric_literal("1__0"), None);
        assert_eq!(split_numeric_literal("0x_1"), None);
        assert_eq!(split_numeric_literal("1z"), None);
    }

    #[test]
    fn separators_do_not_change_values() {
        assert_eq!(parse_int_literal("1_000_000"), Some(1_000_000));
        assert_eq!(parse_u64_literal("0xFFFF_FFFF_FFFF_FFFF"), Some(u64::MAX));
        assert_eq!(parse_signed_literal("0b1111_0000", 32), Some(240));
        assert_eq!(parse_float_literal("1_000.25"), Some(1000.25));
    }
}
