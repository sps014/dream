//! Shared guest callbacks and icon state, owned by one core cdylib.

mod exports;

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_core_abi_v2() {}

#[cfg(test)]
mod contract_tests;
