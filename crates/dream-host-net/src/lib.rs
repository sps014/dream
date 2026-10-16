//! Native URL services; guest allocation remains owned by the core capability.

mod exports;
mod http;
mod http_server;
mod oidc;
mod socket_operations;
mod sockets;
mod urls;
mod websocket;

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_net_abi_v3() {}
