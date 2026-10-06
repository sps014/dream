//! Optional native process services. Guest state remains in the core cdylib.

mod exports;
mod process_host;

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_process_abi_v2() {}
