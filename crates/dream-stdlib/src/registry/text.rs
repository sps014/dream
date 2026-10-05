use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.text",
    deps: &["system.core", "system.primitives", "system.collections"],
    files: &[
        (
            "<std>/system/text/string.dream",
            include_str!("../system/text/string.dream"),
        ),
        (
            "<std>/system/text/unicode_norm_form.dream",
            include_str!("../system/text/unicode_norm_form.dream"),
        ),
        (
            "<std>/system/text/unicode.dream",
            include_str!("../system/text/unicode.dream"),
        ),
        (
            "<std>/system/text/string_iterator.dream",
            include_str!("../system/text/string_iterator.dream"),
        ),
        (
            "<std>/system/text/fmt.dream",
            include_str!("../system/text/fmt.dream"),
        ),
        (
            "<std>/system/text/regex_match_info.dream",
            include_str!("../system/text/regex_match_info.dream"),
        ),
        (
            "<std>/system/text/regex_match.dream",
            include_str!("../system/text/regex_match.dream"),
        ),
        (
            "<std>/system/text/regex_flags.dream",
            include_str!("../system/text/regex_flags.dream"),
        ),
        (
            "<std>/system/text/regex.dream",
            include_str!("../system/text/regex.dream"),
        ),
    ],
};
