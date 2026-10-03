//! Native gpu capability; guest binding state is owned by the core library.
use dream_host_gui as app_icon;
mod exports;
mod gpu;
#[no_mangle]
pub extern "C" fn dream_host_gpu_abi_v2() {}
