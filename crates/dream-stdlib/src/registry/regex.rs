use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.regex",
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
            "<std>/system/regex/regex_error.dream",
            include_str!("../system/regex/regex_error.dream"),
        ),
        (
            "<std>/system/regex/regex_match_info.dream",
            include_str!("../system/regex/regex_match_info.dream"),
        ),
        (
            "<std>/system/regex/regex_match.dream",
            include_str!("../system/regex/regex_match.dream"),
        ),
        (
            "<std>/system/regex/regex_flags.dream",
            include_str!("../system/regex/regex_flags.dream"),
        ),
        (
            "<std>/system/regex/regex.dream",
            include_str!("../system/regex/regex.dream"),
        ),
    ],
};
