//! Device lifetime: strict backend selection, feature reporting and loss checks.

use crate::backend::{self, Backend};
use crate::error::{self, HostResult};
use crate::ffi;
use crate::state::{self, DeviceCore, Entry, Kind};
use crate::waiter;
use std::sync::Arc;

/// Opens one backend: `Err` explains why this machine (or this build) cannot run it.
fn open(backend: Backend) -> Result<*mut ffi::DgpuDevice, String> {
    if unsafe { ffi::dgpu_backend_compiled(backend as i32) } == 0 {
        return Err(format!("this host was built without {}", backend.name()));
    }
    let mut raw = std::ptr::null_mut();
    match unsafe { ffi::dgpu_device_create(backend as i32, &mut raw) } {
        ffi::OK => Ok(raw),
        _ => Err(ffi::message()),
    }
}

/// Creates a device on a completion-free thread and resolves `future` with its handle (0 on
/// failure). Device creation can take long enough to stall a frame, so it never runs on the
/// guest thread.
pub fn create(future: usize, requested: i32, compiled: i32) {
    std::thread::spawn(move || {
        let handle = create_now(requested, compiled as u32).unwrap_or_else(|e| {
            error::record(&e);
            0
        });
        waiter::complete(future, handle as u64);
    });
}

pub fn create_now(requested: i32, compiled: u32) -> HostResult<i32> {
    crate::strict_aot::check();
    let (backend, raw) = backend::select(
        requested,
        compiled,
        backend::auto_order(std::env::consts::OS),
        open,
    )?;
    crate::strict_aot::check();
    let core = Arc::new(DeviceCore::new(raw, backend));
    let mut state = state::lock();
    let handle = state.insert(Entry {
        kind: Kind::Device,
        device: 0,
        last_use: 0,
        core,
    })?;
    state.entry_mut(handle)?.device = handle;
    Ok(handle)
}

pub fn backend(device: i32) -> HostResult<i32> {
    Ok(state::lock().device(device)?.backend as i32)
}

pub fn features(device: i32) -> HostResult<i32> {
    Ok(state::lock().device(device)?.features as i32)
}

/// `Err(EDEVICELOST)` once the device stopped working.
pub fn check(device: i32) -> HostResult<()> {
    state::lock().device(device).map(|_| ())
}
