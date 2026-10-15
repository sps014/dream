//! Native URL services; guest allocation remains owned by the core capability.

mod exports;
mod http;
mod urls;
mod sockets;
mod socket_operations;

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_net_abi_v2() {}
