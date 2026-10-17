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
            "<std>/system/json/json_kind.dream",
            include_str!("../system/json/json_kind.dream"),
        ),
        (
            "<std>/system/json/json_value.dream",
            include_str!("../system/json/json_value.dream"),
        ),
        (
            "<std>/system/json/json_storage.dream",
            include_str!("../system/json/json_storage.dream"),
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
            "<std>/system/json/json_attribute.dream",
            include_str!("../system/json/json_attribute.dream"),
        ),
        (
            "<std>/system/json/property_name_attribute.dream",
            include_str!("../system/json/property_name_attribute.dream"),
        ),
        (
            "<std>/system/json/json_ignore_attribute.dream",
            include_str!("../system/json/json_ignore_attribute.dream"),
        ),
    ],
};
