//! Native host implementations and guest C ABI, separate from the compiler.

mod crypto;
mod exports;
mod process_host;
mod tz;

#[no_mangle]
pub extern "C" fn dream_host_core_abi_v2() {}

#[cfg(test)]
mod contract_tests;
