//! Native URL services; guest allocation remains owned by the core capability.

mod exports;
mod http;
mod urls;

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_net_abi_v2() {}
