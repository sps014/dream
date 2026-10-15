//! Optional native process services. Guest state remains in the core cdylib.

mod exports;
mod operations;
mod process_host;
mod service;

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_process_abi_v2() {}

#[cfg(test)]
mod service_tests;
