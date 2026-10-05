//! Optional native crypto services. Guest state remains in the core cdylib.

mod crypto;
mod exports;

#[no_mangle]
pub extern "C" fn dream_host_crypto_abi_v2() {}
