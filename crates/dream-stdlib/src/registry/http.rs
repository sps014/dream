use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.http",
    deps: &[
        "system.core",
        "system.primitives",
        "system.collections",
        "system.text",
        "system.encoding",
        "system.io",
        "system.json",
    ],
    core_language: false,
    web_assets: None,
    tools: &[],
    guest_exports: &[],
    generators: &[],
    files: &[
        (
            "<std>/system/http/cookie_jar.dream",
            include_str!("../system/http/cookie_jar.dream"),
        ),
        (
            "<std>/system/http/http_method.dream",
            include_str!("../system/http/http_method.dream"),
        ),
        (
            "<std>/system/http/web_socket_operation.dream",
            include_str!("../system/http/web_socket_operation.dream"),
        ),
        (
            "<std>/system/http/web_socket.dream",
            include_str!("../system/http/web_socket.dream"),
        ),
        (
            "<std>/system/http/web_socket_action.dream",
            include_str!("../system/http/web_socket_action.dream"),
        ),
        (
            "<std>/system/http/web_socket_message.dream",
            include_str!("../system/http/web_socket_message.dream"),
        ),
        (
            "<std>/system/http/http_error.dream",
            include_str!("../system/http/http_error.dream"),
        ),
        (
            "<std>/system/http/http_options.dream",
            include_str!("../system/http/http_options.dream"),
        ),
        (
            "<std>/system/http/http_headers.dream",
            include_str!("../system/http/http_headers.dream"),
        ),
        (
            "<std>/system/http/http_request.dream",
            include_str!("../system/http/http_request.dream"),
        ),
        (
            "<std>/system/http/http_capabilities.dream",
            include_str!("../system/http/http_capabilities.dream"),
        ),
        (
            "<std>/system/http/http_client.dream",
            include_str!("../system/http/http_client.dream"),
        ),
        (
            "<std>/system/http/http_operation.dream",
            include_str!("../system/http/http_operation.dream"),
        ),
        (
            "<std>/system/http/http_response.dream",
            include_str!("../system/http/http_response.dream"),
        ),
        (
            "<std>/system/http/http_body.dream",
            include_str!("../system/http/http_body.dream"),
        ),
        (
            "<std>/system/http/url_text.dream",
            include_str!("../system/http/url_text.dream"),
        ),
        (
            "<std>/system/http/url_host.dream",
            include_str!("../system/http/url_host.dream"),
        ),
        (
            "<std>/system/http/url_parse_state.dream",
            include_str!("../system/http/url_parse_state.dream"),
        ),
        (
            "<std>/system/http/url_parser.dream",
            include_str!("../system/http/url_parser.dream"),
        ),
        (
            "<std>/system/http/url.dream",
            include_str!("../system/http/url.dream"),
        ),
        (
            "<std>/system/http/url_query.dream",
            include_str!("../system/http/url_query.dream"),
        ),
    ],
};
