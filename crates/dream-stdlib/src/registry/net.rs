use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.net",
    deps: &[
        "system.core",
        "system.primitives",
        "system.collections",
        "system.io",
    ],
    core_language: false,
    web_assets: None,
    tools: &[],
    guest_exports: &[],
    generators: &[],
    files: &[
        (
            "<std>/system/net/socket_call.dream",
            include_str!("../system/net/socket_call.dream"),
        ),
        (
            "<std>/system/net/socket_state.dream",
            include_str!("../system/net/socket_state.dream"),
        ),
        (
            "<std>/system/net/tcp_stream.dream",
            include_str!("../system/net/tcp_stream.dream"),
        ),
        (
            "<std>/system/net/tcp_listener.dream",
            include_str!("../system/net/tcp_listener.dream"),
        ),
        (
            "<std>/system/net/udp_datagram.dream",
            include_str!("../system/net/udp_datagram.dream"),
        ),
        (
            "<std>/system/net/udp_socket.dream",
            include_str!("../system/net/udp_socket.dream"),
        ),
    ],
};
