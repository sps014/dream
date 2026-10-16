use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.validation",
    deps: &["system", "system.json", "system.text", "system.collections"],
    generators: &["system.validation.derive"],
    files: &[
        (
            "<std>/system/validation/validation_attributes.dream",
            include_str!("../system/validation/validation_attributes.dream"),
        ),
        (
            "<std>/system/validation/validation_errors.dream",
            include_str!("../system/validation/validation_errors.dream"),
        ),
        (
            "<std>/system/validation/validation.dream",
            include_str!("../system/validation/validation.dream"),
        ),
    ],
};
