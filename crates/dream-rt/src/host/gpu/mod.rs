//! wgpu host (same logic wasmtime used). C ABI in [`ffi`].

pub mod abi;
pub mod buffers;
pub mod compute;
pub mod device;
pub mod error;
pub mod gamepad;
pub mod icon;
pub mod input;
pub mod profile;
pub mod render;
pub mod state;
pub mod surface;
pub mod textures;

mod ffi;

use std::path::Path;

pub use icon::set_packaged_app_icon;

/// Load sibling `.abi.json` `gpu` section (WGSL kernels/shaders).
pub fn attach_abi_from_path(path: &str) {
    let p = Path::new(path);
    let gpu = abi::load_gpu_abi_beside(p);
    let abi_path = abi::abi_json_path(p);
    let missing = if gpu.is_some() {
        None
    } else if !abi_path.exists() {
        Some(format!("sibling ABI missing ({})", abi_path.display()))
    } else {
        Some(format!("no `gpu` section in {}", abi_path.display()))
    };
    let mut st = state::lock_state();
    st.reset();
    st.abi = gpu;
    st.missing_gpu_abi = missing;
    st.warned_missing_gpu_abi = false;
}

pub fn load_abi_from_env() {
    use std::sync::OnceLock;
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        if let Ok(p) = std::env::var("DREAM_ABI_JSON") {
            attach_abi_from_path(&p);
            super::c_ffi::attach_from_path(&p);
        } else if let Ok(exe) = std::env::current_exe() {
            let s = exe.to_str().unwrap_or("");
            attach_abi_from_path(s);
            super::c_ffi::attach_from_path(s);
        }
    });
}
