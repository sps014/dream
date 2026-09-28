//! Lexical formatting shared by every IR printer: identifiers, string blobs, float literals.

/// An identifier body for `@name` / `%name`: bare when LLVM's unquoted grammar accepts it,
/// otherwise quoted with `\XX` escapes.
pub fn ident(name: &str) -> String {
    let bare = !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'$' | b'-'))
        && !name.as_bytes()[0].is_ascii_digit();
    if bare {
        return name.to_string();
    }
    let mut out = String::with_capacity(name.len() + 2);
    out.push('"');
    for b in name.bytes() {
        if b == b'"' || b == b'\\' || !(0x20..0x7f).contains(&b) {
            out.push_str(&format!("\\{b:02X}"));
        } else {
            out.push(b as char);
        }
    }
    out.push('"');
    out
}

pub fn global(name: &str) -> String {
    format!("@{}", ident(name))
}

/// `c"..."` byte-string initializer (no implicit NUL; callers append one when needed).
pub fn c_string(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() + 3);
    out.push_str("c\"");
    for &b in bytes {
        if b == b'"' || b == b'\\' || !(0x20..0x7f).contains(&b) {
            out.push_str(&format!("\\{b:02X}"));
        } else {
            out.push(b as char);
        }
    }
    out.push('"');
    out
}

/// LLVM spells `double` and `float` literals as the 64-bit IEEE pattern of the value (a `float`
/// constant must be exactly representable, which widening guarantees).
pub fn f64_lit(v: f64) -> String {
    format!("0x{:016X}", v.to_bits())
}

pub fn f32_lit(v: f32) -> String {
    f64_lit(v as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_quote_only_when_needed() {
        assert_eq!(ident("dream_retain"), "dream_retain");
        assert_eq!(ident("a.b$c-1"), "a.b$c-1");
        assert_eq!(ident("1x"), "\"1x\"");
        assert_eq!(ident("a b"), "\"a b\"");
        assert_eq!(ident("q\"x"), "\"q\\22x\"");
    }

    #[test]
    fn strings_escape_non_printables() {
        assert_eq!(c_string(b"hi\n\0"), "c\"hi\\0A\\00\"");
    }

    #[test]
    fn floats_use_double_bit_patterns() {
        assert_eq!(f64_lit(1.0), "0x3FF0000000000000");
        assert_eq!(f32_lit(0.5), "0x3FE0000000000000");
    }
}
