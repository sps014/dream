//! `DREAM_GPU_STRICT_AOT=1` turns the loader check into a hard assertion for test runs: once a
//! Slang compiler image has been mapped into the process, the next device creation, bundle load
//! or pipeline creation ends the process with the image's path. Without the variable the check
//! never runs, because a program may load Slang for its own purposes.

use crate::ffi;
use std::sync::OnceLock;

pub const VARIABLE: &str = "DREAM_GPU_STRICT_AOT";

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os(VARIABLE).is_some_and(|v| v != "0"))
}

pub fn check() {
    if !enabled() {
        return;
    }
    if unsafe { ffi::dgpu_loaded_slang_compilers() } > 0 {
        eprintln!(
            "dream-host-gpu: {VARIABLE}: the Slang compiler was loaded into this process: {}",
            ffi::message()
        );
        std::process::abort();
    }
}
