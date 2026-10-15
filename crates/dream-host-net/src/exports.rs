use dream_host_abi::{DreamPtr, alloc_string_array, read_string};

/// # Safety
/// Both arguments must be readable guest string payloads for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn netUrlParse(input: DreamPtr, base: DreamPtr) -> DreamPtr {
    let (input, base) = unsafe { (read_string(input), read_string(base)) };
    alloc_string_array(&crate::urls::parse(&input, &base).unwrap_or_default())
}

#[unsafe(no_mangle)]
pub extern "C" fn netClientCreate() -> i32 {
    crate::http::client_create()
}
#[unsafe(no_mangle)]
pub extern "C" fn netClientClose(id: i32) {
    crate::http::client_close(id);
}
/// # Safety
/// Request and body must be valid readable guest string and byte array payloads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn netHttpStart(id: i32, request: DreamPtr, body: DreamPtr) -> i32 {
    let (request, bytes) = unsafe { (read_string(request), dream_host_abi::read_bytes(body)) };
    crate::http::send(id, &request, bytes)
}
#[unsafe(no_mangle)]
pub extern "C" fn netBodyRead(id: i32, count: i32) -> i32 {
    crate::http::read(id, count)
}
#[unsafe(no_mangle)]
pub extern "C" fn netBodyClose(id: i32) {
    crate::http::body_close(id);
}
#[unsafe(no_mangle)]
pub extern "C" fn netOperationPoll(id: i32) -> DreamPtr {
    dream_host_abi::alloc_bytes(&crate::http::poll(id))
}
#[unsafe(no_mangle)]
pub extern "C" fn netOperationRelease(id: i32) {
    crate::http::release(id);
}

#[unsafe(no_mangle)]
pub extern "C" fn netOperationClaim(id: i32) {
    crate::http::claim(id);
}

#[unsafe(no_mangle)]
pub extern "C" fn netCapabilities() -> i32 {
    7
}

#[unsafe(no_mangle)]
pub extern "C" fn netOperationWaitAsync(future: DreamPtr, id: i32) -> i32 {
    if !crate::http::wait(id, dream_host_abi::ForeignFuture::new(future)) {
        dream_host_abi::complete_foreign_future(dream_host_abi::ForeignFuture::new(future), 0);
    }
    1
}
