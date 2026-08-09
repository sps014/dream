//! Escape Dream identifiers that are illegal as WGSL names.

/// Map a Dream identifier to a legal WGSL identifier.
///
/// WGSL forbids reserved keywords (`target`, `input`, …) and names starting with `__`.
/// Escaping is deterministic and stable across emits so joined modules stay consistent.
pub(super) fn escape_wgsl_ident(name: &str) -> String {
    if name.starts_with("__") {
        return format!("_d_{name}");
    }
    if naga::keywords::wgsl::RESERVED_SET.contains(name) {
        return format!("{name}_");
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::escape_wgsl_ident;

    #[test]
    fn reserved_keyword_gets_suffix() {
        assert_eq!(escape_wgsl_ident("target"), "target_");
        assert_eq!(escape_wgsl_ident("input"), "input_");
        assert_eq!(escape_wgsl_ident("output"), "output_");
    }

    #[test]
    fn double_underscore_prefix_rewritten() {
        assert_eq!(escape_wgsl_ident("__vi"), "_d___vi");
    }

    #[test]
    fn ordinary_names_unchanged() {
        assert_eq!(escape_wgsl_ident("eye"), "eye");
        assert_eq!(escape_wgsl_ident("position"), "position");
        assert_eq!(escape_wgsl_ident("_vi"), "_vi");
    }
}
