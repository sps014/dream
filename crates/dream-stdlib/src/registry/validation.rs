use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.validation",
    deps: &["system", "system.json", "system.text", "system.collections"],
    generators: &["system.validation.derive"],
    files: &[
        (
            "<std>/system/validation/minimum_attribute.dream",
            include_str!("../system/validation/minimum_attribute.dream"),
        ),
        (
            "<std>/system/validation/maximum_attribute.dream",
            include_str!("../system/validation/maximum_attribute.dream"),
        ),
        (
            "<std>/system/validation/min_length_attribute.dream",
            include_str!("../system/validation/min_length_attribute.dream"),
        ),
        (
            "<std>/system/validation/max_length_attribute.dream",
            include_str!("../system/validation/max_length_attribute.dream"),
        ),
        (
            "<std>/system/validation/pattern_attribute.dream",
            include_str!("../system/validation/pattern_attribute.dream"),
        ),
        (
            "<std>/system/validation/default_json_attribute.dream",
            include_str!("../system/validation/default_json_attribute.dream"),
        ),
        (
            "<std>/system/validation/allow_extra_attribute.dream",
            include_str!("../system/validation/allow_extra_attribute.dream"),
        ),
        (
            "<std>/system/validation/validate_with_attribute.dream",
            include_str!("../system/validation/validate_with_attribute.dream"),
        ),
        (
            "<std>/system/validation/model_validator_attribute.dream",
            include_str!("../system/validation/model_validator_attribute.dream"),
        ),
        (
            "<std>/system/validation/validation_issue.dream",
            include_str!("../system/validation/validation_issue.dream"),
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
