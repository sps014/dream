//! Optional native unicode services. Guest state remains in the core cdylib.

mod exports;

#[no_mangle]
pub extern "C" fn dream_host_unicode_abi_v2() {}
