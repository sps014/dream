//! Synchronous cross-capability producer callback. Handles are borrowed for the call;
//! each API must retain imported GPU objects through its queued work.
use std::ffi::{c_char, c_void};

pub const COPY_SYMBOL: &str = "dream_gpu_drawing_copy_v1";

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Target {
    /// Process-unique generation of the texture/fence pair; never recycled.
    pub id: u64,
    pub adapter: u64,
    pub texture: usize,
    pub fence: usize,
    pub wait_value: u64,
    pub signal_value: u64,
    pub width: u32,
    pub height: u32,
    pub format: u32,
}

/// 1 submitted, 0 unsupported, -1 error (UTF-8 detail in the caller's buffer).
pub type Producer = unsafe extern "C" fn(*mut c_void, *const Target, *mut c_char, usize) -> i32;
pub type CopyFn = unsafe extern "C" fn(i32, Producer, *mut c_void, *mut c_char, usize) -> i32;

/// # Safety
/// `output` points to `capacity` writable bytes for this call.
pub unsafe fn write_error(output: *mut c_char, capacity: usize, error: &str) {
    if capacity == 0 || output.is_null() {
        return;
    }
    let size = error.len().min(capacity - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(error.as_ptr(), output.cast(), size);
        output.add(size).write(0);
    }
}
