#[unsafe(no_mangle)]
pub extern "C" fn data_alloc(length: usize) -> *mut u8 {
    Box::into_raw(vec![0u8; length].into_boxed_slice()).cast()
}

/// # Safety
/// Pointer and length must identify a live allocation returned by this module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn data_free(pointer: *mut u8, length: usize) {
    unsafe {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
            pointer, length,
        )))
    }
}

/// # Safety
/// The request slice must remain readable until the returned buffer is copied.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn data_parse(pointer: *const u8, length: usize) -> u64 {
    let bytes = unsafe { std::slice::from_raw_parts(pointer, length) };
    let output = match std::str::from_utf8(bytes) {
        Ok(request) => crate::codecs::request(request),
        Err(_) => crate::codecs::failure("request must be UTF-8"),
    };
    let bytes = output.into_bytes().into_boxed_slice();
    let length = bytes.len() as u64;
    let pointer = Box::into_raw(bytes).cast::<u8>() as u64;
    (pointer << 32) | length
}
