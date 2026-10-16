use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.webapi.auth",
    deps: &[
        "system.webapi",
        "system.crypto",
        "system.json",
        "system.encoding",
        "system",
        "system.collections",
    ],
    generators: &[],
    files: &[
        (
            "<std>/system/webapi/auth/auth_types.dream",
            include_str!("../system/webapi/auth/auth_types.dream"),
        ),
        (
            "<std>/system/webapi/auth/auth_crypto.dream",
            include_str!("../system/webapi/auth/auth_crypto.dream"),
        ),
        (
            "<std>/system/webapi/auth/memory_auth_store.dream",
            include_str!("../system/webapi/auth/memory_auth_store.dream"),
        ),
        (
            "<std>/system/webapi/auth/auth_service.dream",
            include_str!("../system/webapi/auth/auth_service.dream"),
        ),
        (
            "<std>/system/webapi/auth/auth_routes.dream",
            include_str!("../system/webapi/auth/auth_routes.dream"),
        ),
    ],
};
