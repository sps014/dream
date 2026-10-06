use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.json",
    deps: &[
        "system.core",
        "system.primitives",
        "system.collections",
        "system.text",
    ],
    generators: &["system.json.derive"],
    files: &[
        (
            "<std>/system/json/json_value.dream",
            include_str!("../system/json/json_value.dream"),
        ),
        (
            "<std>/system/json/json_parser.dream",
            include_str!("../system/json/json_parser.dream"),
        ),
        (
            "<std>/system/json/json.dream",
            include_str!("../system/json/json.dream"),
        ),
        (
            "<std>/system/json/json_attributes.dream",
            include_str!("../system/json/json_attributes.dream"),
        ),
    ],
};
