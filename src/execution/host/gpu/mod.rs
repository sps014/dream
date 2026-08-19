//! Native wgpu host for `system.gpu` (libdream C ABI).

mod abi;
pub(crate) mod buffers;
pub(crate) mod compute;
pub(crate) mod device;
pub(crate) mod error;
mod gamepad;
mod icon;
mod input;
pub(crate) mod profile;
pub(crate) mod render;
mod state;
pub(crate) mod surface;
pub(crate) mod textures;

pub use icon::set_packaged_app_icon;

use std::path::Path;

use state::lock_state;

pub(crate) fn is_ready() -> bool {
    lock_state().ready
}

/// Load sibling `.abi.json` `gpu` section (`DREAM_NATIVE_C` path / compile artifact).
pub fn attach_abi_from_wat_path(wat_path: &str) {
    let path = Path::new(wat_path);
    let abi_path = path.with_extension("abi.json");
    let gpu = abi::load_gpu_abi_beside(path);
    let missing = if gpu.is_some() {
        None
    } else if !abi_path.exists() {
        Some(format!("sibling ABI missing ({})", abi_path.display()))
    } else {
        Some(format!("no `gpu` section in {}", abi_path.display()))
    };
    let mut st = lock_state();
    st.reset();
    st.abi = gpu;
    st.missing_gpu_abi = missing;
    st.warned_missing_gpu_abi = false;
}
