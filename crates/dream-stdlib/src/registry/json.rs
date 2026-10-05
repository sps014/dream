use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.json",
    deps: &[
        "system.core",
        "system.primitives",
        "system.collections",
        "system.text",
        "system.codegen",
    ],
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
            "<std>/system/json/gen_field.dream",
            include_str!("../system/json/gen_field.dream"),
        ),
        (
            "<std>/system/json/gen_collection.dream",
            include_str!("../system/json/gen_collection.dream"),
        ),
        (
            "<std>/system/json/gen_variant.dream",
            include_str!("../system/json/gen_variant.dream"),
        ),
        (
            "<std>/system/json/gen_type.dream",
            include_str!("../system/json/gen_type.dream"),
        ),
        (
            "<std>/system/json/gen_result.dream",
            include_str!("../system/json/gen_result.dream"),
        ),
        (
            "<std>/system/json/json_generator.dream",
            include_str!("../system/json/json_generator.dream"),
        ),
    ],
};
