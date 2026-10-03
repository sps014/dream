//! Native webview capability; guest binding state is owned by the core library.
use dream_host_gui as app_icon;
mod desktop;
mod exports;
mod webview;
#[no_mangle]
pub extern "C" fn dream_host_webview_abi_v2() {}
