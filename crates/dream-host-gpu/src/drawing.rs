//! GPU-only copy handoff; the caller supplies pixels through a synchronous producer callback.
use crate::{
    error::{self, HostError, HostResult},
    ffi,
    state::{self, Kind},
};
use dream_host_abi::drawing_gpu::{Producer, write_error};
use std::ffi::{c_char, c_void};

unsafe fn copy(
    texture: i32,
    producer: Producer,
    context: *mut c_void,
    error: *mut c_char,
    capacity: usize,
) -> HostResult<bool> {
    let mut state = state::lock();
    let entry = state.entry(texture)?;
    entry.core.alive()?;
    let Kind::Texture(target) = &entry.kind else {
        return Err(HostError::invalid("drawing target is not a texture"));
    };
    let core = entry.core.clone();
    let mut fence = 0;
    let status = unsafe {
        ffi::dgpu_drawing_copy(
            core.raw(),
            target.obj.raw(),
            producer,
            context,
            error,
            capacity,
            &mut fence,
        )
    };
    if status == ffi::UNSUPPORTED {
        return Ok(false);
    }
    error::check(status)?;
    state.touch(texture, fence);
    core.push(fence, 0, Box::new(|_, _| 0));
    Ok(true)
}

/// # Safety
/// The producer/context and error buffer must remain valid for this synchronous call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dream_gpu_drawing_copy_v1(
    texture: i32,
    producer: Producer,
    context: *mut c_void,
    output: *mut c_char,
    capacity: usize,
) -> i32 {
    if capacity > 0 && !output.is_null() {
        unsafe { output.write(0) };
    }
    match unsafe { copy(texture, producer, context, output, capacity) } {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(error) => {
            if capacity > 0 && !output.is_null() && unsafe { output.read() } == 0 {
                unsafe { write_error(output, capacity, &error.text()) };
            }
            -1
        }
    }
}
