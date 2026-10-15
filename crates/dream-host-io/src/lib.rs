//! Secure OS temporary resources; guest allocation and binding remain owned by core.
mod exports;
#[unsafe(no_mangle)]
pub extern "C" fn dream_host_io_abi_v2() {}
