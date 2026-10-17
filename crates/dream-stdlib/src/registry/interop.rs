use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.interop",
    deps: &["system.core", "system"],
    generators: &[],
    files: &[
        (
            "<std>/system/interop/c_call_conv.dream",
            include_str!("../system/interop/c_call_conv.dream"),
        ),
        (
            "<std>/system/interop/marshal_kind.dream",
            include_str!("../system/interop/marshal_kind.dream"),
        ),
        (
            "<std>/system/interop/c_attribute.dream",
            include_str!("../system/interop/c_attribute.dream"),
        ),
        (
            "<std>/system/interop/c_call_attribute.dream",
            include_str!("../system/interop/c_call_attribute.dream"),
        ),
        (
            "<std>/system/interop/marshal_attribute.dream",
            include_str!("../system/interop/marshal_attribute.dream"),
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
