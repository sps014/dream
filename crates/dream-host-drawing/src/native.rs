use super::{NativeSurface, SURFACES, fail};
use dream_host_abi::drawing_gpu::{COPY_SYMBOL, CopyFn, Target, write_error};
use std::{
    ffi::{CStr, CString, c_char, c_void},
    sync::OnceLock,
};

static COPY: OnceLock<Option<CopyFn>> = OnceLock::new();

fn lookup() -> Option<CopyFn> {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
    let module = unsafe { GetModuleHandleA(c"dream_host_gpu.dll".as_ptr().cast()) };
    if module.is_null() {
        return None;
    }
    let symbol = CString::new(COPY_SYMBOL).ok()?;
    unsafe { GetProcAddress(module, symbol.as_ptr().cast()) }
        .map(|f| unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, CopyFn>(f) })
}

unsafe extern "C" fn produce(
    context: *mut c_void,
    target: *const Target,
    output: *mut c_char,
    capacity: usize,
) -> i32 {
    let surface = unsafe { &mut *context.cast::<NativeSurface>() };
    match unsafe {
        surface
            .surface
            .copy_native(&*target, surface.picture.as_ref())
    } {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(error) => {
            unsafe { write_error(output, capacity, &error.to_string()) };
            -1
        }
    }
}

pub(super) fn copy(handle: i32, texture: i32, width: i32, height: i32) -> i32 {
    let Some(copy) = COPY.get_or_init(lookup) else {
        return -1;
    };
    let mut surfaces = dream_host_abi::lock(&SURFACES);
    let Some(surface) = surfaces.get_mut(handle) else {
        return fail("EINVAL\ndrawing surface is closed");
    };
    if width <= 0 || height <= 0 {
        return fail("EINVAL\ninvalid drawing dimensions");
    }
    if surface.surface.backend() != 2 {
        return -1;
    }
    let mut error = [0 as c_char; 2048];
    let status = unsafe {
        copy(
            texture,
            produce,
            (surface as *mut NativeSurface).cast(),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    match status {
        1 => 1,
        0 => -1,
        _ => fail(unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy()),
    }
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;
