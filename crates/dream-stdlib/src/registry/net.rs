use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.net",
    deps: &[
        "system.core",
        "system.primitives",
        "system.collections",
        "system.text",
        "system.encoding",
        "system.io",
        "system.json",
    ],
    generators: &[],
    files: &[
        (
            "<std>/system/net/websocket.dream",
            include_str!("../system/net/websocket.dream"),
        ),
        (
            "<std>/system/net/websocket_message.dream",
            include_str!("../system/net/websocket_message.dream"),
        ),
        (
            "<std>/system/net/socket_operation.dream",
            include_str!("../system/net/socket_operation.dream"),
        ),
        (
            "<std>/system/net/tcp.dream",
            include_str!("../system/net/tcp.dream"),
        ),
        (
            "<std>/system/net/udp.dream",
            include_str!("../system/net/udp.dream"),
        ),
        (
            "<std>/system/net/http_types.dream",
            include_str!("../system/net/http_types.dream"),
        ),
        (
            "<std>/system/net/http_client.dream",
            include_str!("../system/net/http_client.dream"),
        ),
        (
            "<std>/system/net/url.dream",
            include_str!("../system/net/url.dream"),
        ),
    ],
};
