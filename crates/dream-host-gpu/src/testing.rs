//! Hooks for host tests that drive the capability without a guest program.

pub use crate::exports::*;
pub use crate::surfaces::SurfaceFn;

/// Routes future completions to `hook(future, value)` and makes reads allocate guest-layout
/// arrays the test can decode with [`crate::guest::test_bytes`].
pub fn set_completion_hook(hook: fn(usize, u64)) {
    crate::waiter::set_completion_hook(hook);
}

/// Uses `f` instead of looking up the window host's export in the loaded libraries.
pub fn set_window_surface_fn(f: SurfaceFn) {
    crate::surfaces::set_window_surface_fn(f);
}

/// Makes every later use of `device` fail with `EDEVICELOST` and `reason`.
pub fn simulate_device_loss(device: i32, reason: &str) -> bool {
    let state = crate::state::lock();
    match state.handles.get(device) {
        Some(entry) => {
            entry.core.simulate_loss(reason);
            true
        }
        None => false,
    }
}

/// Released resources still waiting for their last submission.
pub fn deferred_releases() -> usize {
    crate::state::lock().deferred_len()
}

/// Whether slang-rhi was built with `backend` (GpuBackend numbering).
pub fn backend_compiled(backend: i32) -> bool {
    unsafe { crate::ffi::dgpu_backend_compiled(backend) != 0 }
}

/// The Slang compiler images mapped into this process (expected: none).
pub fn loaded_slang_compilers() -> (usize, String) {
    let count = unsafe { crate::ffi::dgpu_loaded_slang_compilers() };
    (count as usize, crate::ffi::message())
}

/// Shared native drawing textures created by this device, excluding driver/RHI allocations.
pub fn drawing_allocations(device: i32) -> crate::error::HostResult<u64> {
    let core = crate::state::lock().device(device)?;
    Ok(unsafe { crate::ffi::dgpu_drawing_allocations(core.raw()) })
}
