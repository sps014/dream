use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.webapi",
    deps: &[
        "system",
        "system.net",
        "system.task",
        "system.json",
        "system.encoding",
        "system.text",
        "system.collections",
    ],
    generators: &["system.webapi.derive"],
    files: &[
        (
            "<std>/system/webapi/auth.dream",
            include_str!("../system/webapi/auth.dream"),
        ),
        (
            "<std>/system/webapi/cors.dream",
            include_str!("../system/webapi/cors.dream"),
        ),
        (
            "<std>/system/webapi/http_incoming.dream",
            include_str!("../system/webapi/http_incoming.dream"),
        ),
        (
            "<std>/system/webapi/http_outgoing.dream",
            include_str!("../system/webapi/http_outgoing.dream"),
        ),
        (
            "<std>/system/webapi/http_status.dream",
            include_str!("../system/webapi/http_status.dream"),
        ),
        (
            "<std>/system/webapi/middleware.dream",
            include_str!("../system/webapi/middleware.dream"),
        ),
        (
            "<std>/system/webapi/routing.dream",
            include_str!("../system/webapi/routing.dream"),
        ),
        (
            "<std>/system/webapi/server_ws.dream",
            include_str!("../system/webapi/server_ws.dream"),
        ),
        (
            "<std>/system/webapi/sse_event.dream",
            include_str!("../system/webapi/sse_event.dream"),
        ),
        (
            "<std>/system/webapi/uploaded_file.dream",
            include_str!("../system/webapi/uploaded_file.dream"),
        ),
        (
            "<std>/system/webapi/web_app.dream",
            include_str!("../system/webapi/web_app.dream"),
        ),
        (
            "<std>/system/webapi/web_app_options.dream",
            include_str!("../system/webapi/web_app_options.dream"),
        ),
        (
            "<std>/system/webapi/web_operation.dream",
            include_str!("../system/webapi/web_operation.dream"),
        ),
        (
            "<std>/system/webapi/webapi_attributes.dream",
            include_str!("../system/webapi/webapi_attributes.dream"),
        ),
    ],
};
