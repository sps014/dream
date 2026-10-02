//! Shared GPU host error codes + classification (parity with JS `classifyErr`).

use super::state::{
    lock_state, ERR_DEVICE_LOST, ERR_OTHER, ERR_TIMEOUT, ERR_UNAVAILABLE, ERR_UNSUPPORTED,
    ERR_VALIDATION,
};
use std::cell::RefCell;

thread_local! {
    static LAST_UNCAPTURED: RefCell<Option<String>> = const { RefCell::new(None) };
    static DEVICE_LOST: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Map an error message to a Dream `GpuError` host code.
pub fn classify_err(msg: &str) -> i32 {
    let lower = msg.to_ascii_lowercase();
    if is_device_lost_msg(&lower) {
        ERR_DEVICE_LOST
    } else if lower.contains("unsupported") {
        ERR_UNSUPPORTED
    } else if lower.contains("not initialized")
        || lower.contains("no adapter")
        || lower.contains("unavailable")
        || lower.contains("no window")
        || lower.contains("not available")
    {
        ERR_UNAVAILABLE
    } else if lower.contains("timeout") || lower.contains("timed out") {
        ERR_TIMEOUT
    } else if lower.contains("unknown")
        || lower.contains("empty")
        || lower.contains("abi")
        || lower.contains("wgsl")
        || lower.contains("validation")
        || lower.contains("compile")
        || lower.contains("missing")
        || lower.contains("outdated")
    {
        ERR_VALIDATION
    } else {
        ERR_OTHER
    }
}

fn is_device_lost_msg(lower: &str) -> bool {
    lower.contains("device lost")
        || lower.contains("lost device")
        || lower.contains("parent device is lost")
}

/// Classify a wgpu surface acquire failure.
///
/// Swapchain `Lost` / `Outdated` are recoverable by reconfigure, not a new device — those stay
/// `VALIDATION`. A message that names the *device* as lost is `DEVICE_LOST`.
pub fn classify_surface_error(err: &wgpu::SurfaceError) -> i32 {
    match err {
        wgpu::SurfaceError::Timeout => ERR_TIMEOUT,
        wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost => ERR_VALIDATION,
        wgpu::SurfaceError::OutOfMemory => ERR_OTHER,
        other => classify_err(&other.to_string()),
    }
}

/// wgpu uncaptured-error callback. Must not take the GPU state mutex: it can fire mid-submit.
pub fn note_uncaptured_error(err: wgpu::Error) {
    let msg = err.to_string();
    eprintln!("Dream wgpu error: {msg}");
    if is_device_lost_msg(&msg.to_ascii_lowercase()) {
        note_device_lost(msg);
        return;
    }
    LAST_UNCAPTURED.with(|c| {
        *c.borrow_mut() = Some(msg);
    });
}

pub fn note_device_lost(msg: String) {
    eprintln!("Dream wgpu device lost: {msg}");
    DEVICE_LOST.with(|c| {
        *c.borrow_mut() = Some(msg);
    });
}

pub fn lost_pending() -> bool {
    DEVICE_LOST.with(|c| c.borrow().is_some())
}

pub fn drain_lost() -> Option<String> {
    DEVICE_LOST.with(|c| c.borrow_mut().take())
}

pub fn drain_uncaptured() -> Option<String> {
    LAST_UNCAPTURED.with(|c| c.borrow_mut().take())
}

/// Returns and clears the last GPU host error detail (empty when none). Also drains any pending
/// uncaptured wgpu message so Dream `GpuError` messages stay in sync with stderr.
pub fn take_last_error() -> String {
    if let Some(msg) = drain_lost() {
        lock_state().set_last_error(msg);
    } else if let Some(msg) = drain_uncaptured() {
        lock_state().set_last_error(msg);
    }
    lock_state().last_error.take().unwrap_or_default()
}

/// 0 if the device is alive, otherwise a `GpuError` host code. Consumes a pending lost /
/// uncaptured event so the next `from_code` can attach the detail string.
pub fn poll_status() -> i32 {
    if let Some(msg) = drain_lost() {
        let mut st = lock_state();
        st.drop_gpu_device();
        st.set_last_error(msg);
        return ERR_DEVICE_LOST;
    }
    if let Some(msg) = drain_uncaptured() {
        let code = classify_err(&msg);
        let mut st = lock_state();
        if code == ERR_DEVICE_LOST {
            st.drop_gpu_device();
        }
        st.set_last_error(msg);
        return code;
    }
    let st = lock_state();
    if st.ready && st.device.is_some() {
        0
    } else {
        ERR_UNAVAILABLE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::state::{ERR_DEVICE_LOST, ERR_UNSUPPORTED, ERR_VALIDATION};

    #[test]
    fn device_lost_is_not_validation() {
        assert_eq!(
            classify_err("device lost (Unknown): GPU process crashed"),
            ERR_DEVICE_LOST
        );
        assert_eq!(
            classify_err("Validation Error: parent device is lost"),
            ERR_DEVICE_LOST
        );
        assert_eq!(
            classify_err("validation: unknown kernel"),
            ERR_VALIDATION
        );
        assert_eq!(classify_err("unsupported: texture format"), ERR_UNSUPPORTED);
    }
}
