//! Source-name → valid-C-identifier sanitization shared by every codegen backend.
//!
//! The C backends derive debugger-readable identifiers from MIR/HIR source names; this module owns
//! the character-level rules so all backends agree. Collision policy against synthetic namespaces
//! (`l{n}`, `t{n}`, `__vs*`, …) is per-backend — only lexical validity lives here.

/// Identifiers that cannot be used as plain C identifiers.
const C_KEYWORDS: &[&str] = &[
    "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else",
    "enum", "extern", "float", "for", "goto", "if", "inline", "int", "long", "register",
    "restrict", "return", "short", "signed", "sizeof", "static", "struct", "switch", "typedef",
    "union", "unsigned", "void", "volatile", "while", "_Alignas", "_Alignof", "_Atomic",
    "_Bool", "_Complex", "_Generic", "_Imaginary", "_Noreturn", "_Static_assert",
    "_Thread_local",
];

pub(crate) fn is_c_keyword(s: &str) -> bool {
    C_KEYWORDS.contains(&s)
}

/// Maps a Dream identifier to a lexically-valid, collision-free-with-the-language C identifier:
///
/// - characters outside `[A-Za-z0-9_]` become `_`
/// - leading underscores are dropped (reserved for the C implementation)
/// - a leading digit gets an `_` prefix
/// - C keywords get a trailing `_`
///
/// Returns `None` when nothing usable remains (e.g. the name was entirely underscores).
pub(crate) fn sanitize_ident(name: &str) -> Option<String> {
    let mapped: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    let trimmed = mapped.trim_start_matches('_');
    if trimmed.is_empty() {
        return None;
    }
    let mut out = String::from(trimmed);
    if trimmed.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    if is_c_keyword(trimmed) {
        out.push('_');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_plain_names() {
        assert_eq!(sanitize_ident("count"), Some("count".into()));
        assert_eq!(sanitize_ident("_x"), Some("x".into()));
        assert_eq!(sanitize_ident("__deep"), Some("deep".into()));
    }

    #[test]
    fn replaces_invalid_chars() {
        assert_eq!(sanitize_ident("a-b"), Some("a_b".into()));
        assert_eq!(sanitize_ident("a b!c"), Some("a_b_c".into()));
    }

    #[test]
    fn escapes_digit_leading_and_keywords() {
        assert_eq!(sanitize_ident("1st"), Some("_1st".into()));
        assert_eq!(sanitize_ident("double"), Some("double_".into()));
        assert_eq!(sanitize_ident("return"), Some("return_".into()));
    }

    #[test]
    fn rejects_unusable() {
        assert_eq!(sanitize_ident(""), None);
        assert_eq!(sanitize_ident("___"), None);
        assert_eq!(sanitize_ident("_-_-"), None);
    }
}
