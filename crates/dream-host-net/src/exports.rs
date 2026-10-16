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
    15
}

#[unsafe(no_mangle)]
pub extern "C" fn netOperationWaitAsync(future: DreamPtr, id: i32) -> i32 {
    if !crate::http::wait(id, dream_host_abi::ForeignFuture::new(future)) {
        dream_host_abi::complete_foreign_future(dream_host_abi::ForeignFuture::new(future), 0);
    }
    1
}

/// # Safety
/// Request and bytes must identify readable guest string and byte-array payloads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn netSocketStart(request: DreamPtr, bytes: DreamPtr) -> i32 {
    crate::sockets::start(unsafe { read_string(request) }, unsafe {
        dream_host_abi::read_bytes(bytes)
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn netSocketWaitAsync(future: DreamPtr, id: i32) -> i32 {
    if !crate::socket_operations::wait(id, dream_host_abi::ForeignFuture::new(future)) {
        dream_host_abi::complete_foreign_future(dream_host_abi::ForeignFuture::new(future), 0);
    }
    1
}
#[unsafe(no_mangle)]
pub extern "C" fn netSocketResult(id: i32) -> DreamPtr {
    dream_host_abi::alloc_bytes(&crate::socket_operations::result(id))
}
#[unsafe(no_mangle)]
pub extern "C" fn netSocketCancel(id: i32) {
    crate::socket_operations::cancel(id);
}
#[unsafe(no_mangle)]
pub extern "C" fn netSocketRelease(id: i32) {
    crate::socket_operations::release(id);
}
#[unsafe(no_mangle)]
pub extern "C" fn netSocketClaim(id: i32) {
    crate::socket_operations::claim(id);
}
#[unsafe(no_mangle)]
pub extern "C" fn netSocketClose(id: i32) {
    crate::sockets::close(id);
}
/// # Safety
/// Request must identify a readable guest string for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn netSocketControl(id: i32, request: DreamPtr) -> DreamPtr {
    dream_host_abi::alloc_string(&crate::sockets::control(id, &unsafe {
        read_string(request)
    }))
}

/// # Safety
/// Request and bytes must identify readable guest string and byte-array payloads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn netWsStart(request: DreamPtr, bytes: DreamPtr) -> i32 {
    crate::websocket::start(unsafe { read_string(request) }, unsafe {
        dream_host_abi::read_bytes(bytes)
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn netWsClose(id: i32) {
    crate::websocket::close(id);
}

/// # Safety
/// Request and bytes must identify readable guest string and byte-array payloads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn httpServerStart(request: DreamPtr, bytes: DreamPtr) -> i32 {
    crate::http_server::start(unsafe { read_string(request) }, unsafe {
        dream_host_abi::read_bytes(bytes)
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn httpServerClose(id: i32) {
    crate::http_server::close(id);
}
#[unsafe(no_mangle)]
pub extern "C" fn httpServerReleaseRequest(id: i32) {
    crate::http_server::release(id);
}

#[unsafe(no_mangle)]
pub extern "C" fn httpServerWaitAsync(future: DreamPtr, id: i32) -> i32 {
    netSocketWaitAsync(future, id)
}
#[unsafe(no_mangle)]
pub extern "C" fn httpServerResult(id: i32) -> DreamPtr {
    netSocketResult(id)
}
#[unsafe(no_mangle)]
pub extern "C" fn httpServerCancel(id: i32) {
    netSocketCancel(id);
}
#[unsafe(no_mangle)]
pub extern "C" fn httpServerRelease(id: i32) {
    netSocketRelease(id);
}
#[unsafe(no_mangle)]
pub extern "C" fn httpServerClaim(id: i32) {
    netSocketClaim(id);
}

#[unsafe(no_mangle)]
pub extern "C" fn httpServerShutdown(id: i32) {
    crate::http_server::shutdown(id);
}
