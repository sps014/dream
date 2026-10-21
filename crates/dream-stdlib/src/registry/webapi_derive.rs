use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.webapi.derive",
    deps: &[
        "system.webapi",
        "system.validation.derive",
        "system.codegen",
        "system.json",
        "system.collections",
        "system",
    ],
    generators: &[],
    files: &[
        (
            "<std>/system/webapi/derive/web_generator_deps.dream",
            include_str!("../system/webapi/derive/web_generator_deps.dream"),
        ),
        (
            "<std>/system/webapi/derive/web_generator_dispatch.dream",
            include_str!("../system/webapi/derive/web_generator_dispatch.dream"),
        ),
        (
            "<std>/system/webapi/derive/web_generator_extractors.dream",
            include_str!("../system/webapi/derive/web_generator_extractors.dream"),
        ),
        (
            "<std>/system/webapi/derive/web_generator_middleware.dream",
            include_str!("../system/webapi/derive/web_generator_middleware.dream"),
        ),
        (
            "<std>/system/webapi/derive/web_generator_openapi.dream",
            include_str!("../system/webapi/derive/web_generator_openapi.dream"),
        ),
        (
            "<std>/system/webapi/derive/web_generator_routes.dream",
            include_str!("../system/webapi/derive/web_generator_routes.dream"),
        ),
        (
            "<std>/system/webapi/derive/webapi_derive.dream",
            include_str!("../system/webapi/derive/webapi_derive.dream"),
        ),
        (
            "<std>/system/webapi/derive/web_route.dream",
            include_str!("../system/webapi/derive/web_route.dream"),
        ),
        (
            "<std>/system/webapi/derive/web_generator.dream",
            include_str!("../system/webapi/derive/web_generator.dream"),
        ),
    ],
};
