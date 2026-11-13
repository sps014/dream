use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.interop",
    deps: &["system.core", "system"],
    core_language: true,
    web_assets: None,
    tools: &[],
    guest_exports: &[],
    generators: &[],
    files: &[
        (
            "<std>/system/interop/string_encoding.dream",
            include_str!("../system/interop/string_encoding.dream"),
        ),
        (
            "<std>/system/interop/user_data_position.dream",
            include_str!("../system/interop/user_data_position.dream"),
        ),
        (
            "<std>/system/interop/c_attribute.dream",
            include_str!("../system/interop/c_attribute.dream"),
        ),
        (
            "<std>/system/interop/marshal_attribute.dream",
            include_str!("../system/interop/marshal_attribute.dream"),
        ),
        (
            "<std>/system/interop/user_data_attribute.dream",
            include_str!("../system/interop/user_data_attribute.dream"),
        ),
        (
            "<std>/system/interop/owned_attribute.dream",
            include_str!("../system/interop/owned_attribute.dream"),
        ),
        (
            "<std>/system/interop/packed_attribute.dream",
            include_str!("../system/interop/packed_attribute.dream"),
        ),
        (
            "<std>/system/interop/cpp_attribute.dream",
            include_str!("../system/interop/cpp_attribute.dream"),
        ),
        (
            "<std>/system/interop/cpp_name_attribute.dream",
            include_str!("../system/interop/cpp_name_attribute.dream"),
        ),
    ],
};
