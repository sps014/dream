use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.webapi.derive",
    deps: &[
        "system.webapi",
        "system.codegen",
        "system.json",
        "system.collections",
        "system",
    ],
    generators: &[],
    files: &[
        (
            "<std>/system/webapi/derive/gen_deps.dream",
            include_str!("../system/webapi/derive/gen_deps.dream"),
        ),
        (
            "<std>/system/webapi/derive/gen_dispatch.dream",
            include_str!("../system/webapi/derive/gen_dispatch.dream"),
        ),
        (
            "<std>/system/webapi/derive/gen_extractors.dream",
            include_str!("../system/webapi/derive/gen_extractors.dream"),
        ),
        (
            "<std>/system/webapi/derive/gen_middleware.dream",
            include_str!("../system/webapi/derive/gen_middleware.dream"),
        ),
        (
            "<std>/system/webapi/derive/gen_openapi.dream",
            include_str!("../system/webapi/derive/gen_openapi.dream"),
        ),
        (
            "<std>/system/webapi/derive/gen_routes.dream",
            include_str!("../system/webapi/derive/gen_routes.dream"),
        ),
        (
            "<std>/system/webapi/derive/webapi_derive.dream",
            include_str!("../system/webapi/derive/webapi_derive.dream"),
        ),
    ],
};
