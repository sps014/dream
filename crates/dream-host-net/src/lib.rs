//! Native net capability; guest binding state is owned by the core library.
mod exports;
mod http;
mod http_server;
mod net;

#[no_mangle]
pub extern "C" fn dream_host_net_abi_v2() {}
