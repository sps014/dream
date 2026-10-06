use super::StdPackage;

/// The `@json` derive generator. Registered through `system.json`'s `generators`; only its
/// generator executable compiles it, user programs never do.
pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.json.derive",
    deps: &[
        "system.core",
        "system.primitives",
        "system.collections",
        "system.text",
        "system.json",
        "system.codegen",
    ],
    generators: &[],
    files: &[
        (
            "<std>/system/json/derive/gen_field.dream",
            include_str!("../system/json/derive/gen_field.dream"),
        ),
        (
            "<std>/system/json/derive/gen_collection.dream",
            include_str!("../system/json/derive/gen_collection.dream"),
        ),
        (
            "<std>/system/json/derive/gen_variant.dream",
            include_str!("../system/json/derive/gen_variant.dream"),
        ),
        (
            "<std>/system/json/derive/gen_type.dream",
            include_str!("../system/json/derive/gen_type.dream"),
        ),
        (
            "<std>/system/json/derive/gen_result.dream",
            include_str!("../system/json/derive/gen_result.dream"),
        ),
        (
            "<std>/system/json/derive/json_generator.dream",
            include_str!("../system/json/derive/json_generator.dream"),
        ),
        (
            "<std>/system/json/derive/json_derive.dream",
            include_str!("../system/json/derive/json_derive.dream"),
        ),
    ],
};
