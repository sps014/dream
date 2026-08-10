//! Shared GPU host error codes + classification (parity with JS `classifyErr`).

use super::state::{ERR_OTHER, ERR_TIMEOUT, ERR_UNAVAILABLE, ERR_VALIDATION};
use std::cell::RefCell;

thread_local! {
    static LAST_UNCAPTURED: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Map an error message to a Dream `GpuError` host code.
pub fn classify_err(msg: &str) -> i32 {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("not initialized")
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
        || lower.contains("lost")
    {
        ERR_VALIDATION
    } else {
        ERR_OTHER
    }
}

/// Classify a wgpu surface acquire failure.
pub fn classify_surface_error(err: &wgpu::SurfaceError) -> i32 {
    match err {
        wgpu::SurfaceError::Timeout => ERR_TIMEOUT,
        wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost => ERR_VALIDATION,
        wgpu::SurfaceError::OutOfMemory => ERR_OTHER,
        other => classify_err(&other.to_string()),
    }
}

pub fn note_uncaptured(msg: String) {
    eprintln!("Dream wgpu error: {msg}");
    LAST_UNCAPTURED.with(|c| {
        *c.borrow_mut() = Some(msg);
    });
}

pub fn drain_uncaptured() -> Option<String> {
    LAST_UNCAPTURED.with(|c| c.borrow_mut().take())
}
