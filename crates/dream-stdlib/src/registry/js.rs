use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.js",
    deps: &["system.core", "system.primitives"],
    core_language: true,
    web_assets: None,
    tools: &[],
    guest_exports: &[],
    generators: &[],
    files: &[
        (
            "<std>/system/js/js_type_attribute.dream",
            include_str!("../system/js/js_type_attribute.dream"),
        ),
        (
            "<std>/system/js/js_attribute.dream",
            include_str!("../system/js/js_attribute.dream"),
        ),
        (
            "<std>/system/js/js.dream",
            include_str!("../system/js/js.dream"),
        ),
        (
            "<std>/system/js/js_error.dream",
            include_str!("../system/js/js_error.dream"),
        ),
    ],
};
