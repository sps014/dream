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
            "<std>/system/webapi/auth/auth_error.dream",
            include_str!("../system/webapi/auth/auth_error.dream"),
        ),
        (
            "<std>/system/webapi/auth/auth_principal.dream",
            include_str!("../system/webapi/auth/auth_principal.dream"),
        ),
        (
            "<std>/system/webapi/auth/auth_account.dream",
            include_str!("../system/webapi/auth/auth_account.dream"),
        ),
        (
            "<std>/system/webapi/auth/auth_accounts.dream",
            include_str!("../system/webapi/auth/auth_accounts.dream"),
        ),
        (
            "<std>/system/webapi/auth/auth_session.dream",
            include_str!("../system/webapi/auth/auth_session.dream"),
        ),
        (
            "<std>/system/webapi/auth/oidc_transaction.dream",
            include_str!("../system/webapi/auth/oidc_transaction.dream"),
        ),
        (
            "<std>/system/webapi/auth/auth_session_store.dream",
            include_str!("../system/webapi/auth/auth_session_store.dream"),
        ),
        (
            "<std>/system/webapi/auth/jwt_options.dream",
            include_str!("../system/webapi/auth/jwt_options.dream"),
        ),
        (
            "<std>/system/webapi/auth/oidc_options.dream",
            include_str!("../system/webapi/auth/oidc_options.dream"),
        ),
        (
            "<std>/system/webapi/auth/auth_tokens.dream",
            include_str!("../system/webapi/auth/auth_tokens.dream"),
        ),
        (
            "<std>/system/webapi/auth/auth_crypto.dream",
            include_str!("../system/webapi/auth/auth_crypto.dream"),
        ),
        (
            "<std>/system/webapi/auth/password.dream",
            include_str!("../system/webapi/auth/password.dream"),
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
