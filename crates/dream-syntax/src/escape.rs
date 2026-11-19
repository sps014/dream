//! Backslash escapes in string and character literals: `\n`, `\t`, `\r`, `\0`, `\\`, `\"`, `\'`,
//! and `\u{X}` with one to six hex digits naming a Unicode scalar value.

/// Decodes the escape that follows a backslash. `rest` is the literal text after the backslash.
/// Returns the character and how many bytes of `rest` the escape used.
pub fn decode_escape(rest: &str) -> Result<(char, usize), String> {
    let mut chars = rest.chars();
    let Some(first) = chars.next() else {
        return Err("a backslash at the end of a literal must be written '\\\\'".to_string());
    };
    let simple = match first {
        'n' => Some('\n'),
        't' => Some('\t'),
        'r' => Some('\r'),
        '0' => Some('\0'),
        '\\' => Some('\\'),
        '"' => Some('"'),
        '\'' => Some('\''),
        _ => None,
    };
    if let Some(c) = simple {
        return Ok((c, 1));
    }
    if first != 'u' {
        return Err(format!(
            "unknown escape '\\{first}'; write '\\\\' for a literal backslash"
        ));
    }
    let malformed = || "a '\\u' escape is written '\\u{XXXX}' with 1 to 6 hex digits".to_string();
    let braced = rest[1..].strip_prefix('{').ok_or_else(malformed)?;
    let close = braced.find('}').ok_or_else(malformed)?;
    let digits = &braced[..close];
    if digits.is_empty() || digits.len() > 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(malformed());
    }
    let value = u32::from_str_radix(digits, 16).map_err(|_| malformed())?;
    let c = char::from_u32(value)
        .ok_or_else(|| format!("'\\u{{{digits}}}' is not a Unicode scalar value"))?;
    Ok((c, 1 + 1 + close + 1))
}

/// The literal body with every escape expanded. An invalid escape keeps its text; the parser
/// reports it.
pub fn unescape(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(at) = rest.find('\\') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        match decode_escape(after) {
            Ok((c, used)) => {
                out.push(c);
                rest = &after[used..];
            }
            Err(_) => {
                out.push('\\');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The message for every invalid escape in a literal body.
pub fn escape_errors(body: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let mut rest = body;
    while let Some(at) = rest.find('\\') {
        let after = &rest[at + 1..];
        match decode_escape(after) {
            Ok((_, used)) => rest = &after[used..],
            Err(message) => {
                errors.push(message);
                rest = after.chars().next().map_or("", |c| &after[c.len_utf8()..]);
            }
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_simple_and_unicode_escapes() {
        assert_eq!(unescape(r"a\nb\t\\\u{1F600}\u{41}"), "a\nb\t\\😀A");
        assert!(escape_errors(r#"\"\'\0\r"#).is_empty());
    }

    #[test]
    fn reports_unknown_and_malformed_escapes() {
        assert_eq!(escape_errors(r"\q").len(), 1);
        assert_eq!(escape_errors(r"\uFFFF").len(), 1);
        assert_eq!(escape_errors(r"\u{}").len(), 1);
        assert_eq!(escape_errors(r"\u{1234567}").len(), 1);
        assert_eq!(escape_errors(r"\u{D800}").len(), 1);
        assert_eq!(escape_errors(r"\u{110000}").len(), 1);
        assert_eq!(escape_errors("\\é").len(), 1);
        assert_eq!(unescape(r"\q"), r"\q");
    }
}
