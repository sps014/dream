use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.validation.derive",
    deps: &[
        "system.validation",
        "system.codegen",
        "system.json",
        "system.collections",
        "system",
    ],
    core_language: false,
    web_assets: None,
    tools: &[],
    guest_exports: &[],
    generators: &[],
    files: &[
        (
            "<std>/system/validation/derive/validation_derive.dream",
            include_str!("../system/validation/derive/validation_derive.dream"),
        ),
        (
            "<std>/system/validation/derive/validation_generator.dream",
            include_str!("../system/validation/derive/validation_generator.dream"),
        ),
    ],
};
