use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.unicode",
    deps: &[
        "system.core",
        "system.primitives",
        "system.collections",
        "system.text",
    ],
    core_language: false,
    web_assets: None,
    tools: &[],
    guest_exports: &[],
    generators: &[],
    files: &[
        (
            "<std>/system/unicode/unicode_norm_form.dream",
            include_str!("../system/unicode/unicode_norm_form.dream"),
        ),
        (
            "<std>/system/unicode/unicode_normalization_data.dream",
            include_str!("../system/unicode/unicode_normalization_data.dream"),
        ),
        (
            "<std>/system/unicode/unicode_normalizer.dream",
            include_str!("../system/unicode/unicode_normalizer.dream"),
        ),
        (
            "<std>/system/unicode/grapheme_data.dream",
            include_str!("../system/unicode/grapheme_data.dream"),
        ),
        (
            "<std>/system/unicode/grapheme_breaker.dream",
            include_str!("../system/unicode/grapheme_breaker.dream"),
        ),
        (
            "<std>/system/unicode/string_unicode.dream",
            include_str!("../system/unicode/string_unicode.dream"),
        ),
    ],
};
