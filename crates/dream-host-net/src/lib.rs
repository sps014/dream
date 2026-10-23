//! Native networking and IDNA services; guest allocation remains owned by the core capability.

mod cookies;
mod exports;
mod http;
mod http_server;
mod oidc;
mod socket_operations;
mod sockets;
mod websocket;

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_net_abi_v4() {}
