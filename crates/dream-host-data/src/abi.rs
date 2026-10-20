use dream_abi::capabilities::{
    AbiVersion, CAPABILITY_ABI_VERSION, DataAlloc, DataFree, DataRequest, HostBuffer,
};

const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024;

#[unsafe(no_mangle)]
pub extern "C" fn dream_capability_abi_version() -> u32 {
    CAPABILITY_ABI_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn dream_data_alloc(length: usize) -> *mut u8 {
    if length > MAX_REQUEST_BYTES {
        return std::ptr::null_mut();
    }
    let mut bytes = Vec::<u8>::new();
    if bytes.try_reserve_exact(length).is_err() {
        return std::ptr::null_mut();
    }
    bytes.resize(length, 0);
    Box::into_raw(bytes.into_boxed_slice()).cast()
}

/// # Safety
/// Pointer and length identify a live allocation returned by this capability.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dream_data_free(pointer: *mut u8, length: usize) {
    if !pointer.is_null() {
        unsafe {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                pointer, length,
            )));
        }
    }
}

pub(crate) fn request_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(request) => crate::codecs::request(request),
        Err(_) => crate::codecs::failure("request must be UTF-8"),
    }
}

/// # Safety
/// The request is readable for the call. Output points to writable descriptor storage.
/// The returned bytes belong to this capability and must be freed with their original length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dream_data_request(
    pointer: *const u8,
    length: usize,
    output: *mut HostBuffer,
) {
    if output.is_null() {
        return;
    }
    let response = if length > MAX_REQUEST_BYTES {
        crate::codecs::failure("codec request exceeds 16 MiB")
    } else if length == 0 {
        request_bytes(&[])
    } else if pointer.is_null() {
        crate::codecs::failure("codec request has a null pointer")
    } else {
        request_bytes(unsafe { std::slice::from_raw_parts(pointer, length) })
    };
    let bytes = response.into_bytes().into_boxed_slice();
    let result = HostBuffer {
        length: bytes.len(),
        data: Box::into_raw(bytes).cast(),
    };
    // WASM adapters allocate descriptor bytes without imposing a native struct alignment.
    unsafe { output.write_unaligned(result) };
}

const _: AbiVersion = dream_capability_abi_version;
const _: DataAlloc = dream_data_alloc;
const _: DataFree = dream_data_free;
const _: DataRequest = dream_data_request;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_span_abi_reports_bad_input_without_panicking() {
        let invalid_utf8 = [0xff_u8];
        for (pointer, length, message) in [
            (std::ptr::null(), 0, "invalid codec request"),
            (std::ptr::null(), 1, "null pointer"),
            (std::ptr::null(), MAX_REQUEST_BYTES + 1, "exceeds 16 MiB"),
            (invalid_utf8.as_ptr(), 1, "UTF-8"),
        ] {
            let mut output = HostBuffer {
                data: std::ptr::null_mut(),
                length: 0,
            };
            unsafe {
                dream_data_request(pointer, length, &mut output);
                let result = std::slice::from_raw_parts(output.data, output.length);
                let error: serde_json::Value = serde_json::from_slice(result).unwrap();
                assert!(error["error"].as_str().unwrap().contains(message));
                dream_data_free(output.data, output.length);
            }
        }
        assert_eq!(dream_capability_abi_version(), CAPABILITY_ABI_VERSION);
        assert!(dream_data_alloc(MAX_REQUEST_BYTES + 1).is_null());
    }
}
