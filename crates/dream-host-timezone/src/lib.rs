//! Optional native timezone services. Guest state remains in the core cdylib.

mod exports;
mod tz;

#[no_mangle]
pub extern "C" fn dream_host_timezone_abi_v2() {}
