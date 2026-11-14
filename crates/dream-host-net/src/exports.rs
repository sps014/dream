use dream_host_abi::{DreamPtr, alloc_string, read_string};

/// # Safety
/// `domain` must be a readable guest string payload for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn netDomainToAscii(domain: DreamPtr) -> DreamPtr {
    alloc_string(&url::quirks::domain_to_ascii(&unsafe {
        read_string(domain)
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn netClientCreate(cookie_jar: i32) -> i32 {
    crate::http::client_create(cookie_jar)
}
#[unsafe(no_mangle)]
pub extern "C" fn netCookieJarCreate() -> i32 {
    crate::cookies::create()
}
#[unsafe(no_mangle)]
pub extern "C" fn netCookieJarClose(id: i32) {
    crate::cookies::close(id);
}
/// # Safety
/// `request` must identify a readable guest string for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn netCookieJarControl(id: i32, request: DreamPtr) -> DreamPtr {
    alloc_string(&crate::cookies::control(id, &unsafe {
        read_string(request)
    }))
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
    23
}

#[unsafe(no_mangle)]
pub extern "C" fn netOperationWaitAsync(future: DreamPtr, id: i32) -> i32 {
    if !crate::http::wait(id, dream_host_abi::ForeignFuture::new(future)) {
        dream_host_abi::complete_foreign_future(dream_host_abi::ForeignFuture::new(future), 0);
    }
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn netWsWaitAsync(future: DreamPtr, id: i32) -> i32 {
    if !crate::operations::OPERATIONS.wait(id, dream_host_abi::ForeignFuture::new(future)) {
        dream_host_abi::complete_foreign_future(dream_host_abi::ForeignFuture::new(future), 0);
    }
    1
}
#[unsafe(no_mangle)]
pub extern "C" fn netWsResult(id: i32) -> DreamPtr {
    dream_host_abi::alloc_bytes(&crate::operations::OPERATIONS.result(id))
}
#[unsafe(no_mangle)]
pub extern "C" fn netWsCancel(id: i32) {
    crate::operations::OPERATIONS.cancel(id);
}
#[unsafe(no_mangle)]
pub extern "C" fn netWsRelease(id: i32) {
    crate::operations::OPERATIONS.release(id);
}
#[unsafe(no_mangle)]
pub extern "C" fn netWsClaim(id: i32) {
    crate::operations::OPERATIONS.claim(id);
}

/// # Safety
/// Request and bytes must identify readable guest byte-array payloads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn netWsStart(request: DreamPtr, bytes: DreamPtr) -> i32 {
    crate::websocket::start(unsafe { dream_host_abi::read_bytes(request) }, unsafe {
        dream_host_abi::read_bytes(bytes)
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn netWsClose(id: i32) {
    crate::websocket::close(id);
}

/// # Safety
/// Request and bytes must identify readable guest byte-array payloads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn httpServerStart(request: DreamPtr, bytes: DreamPtr) -> i32 {
    crate::http_server::start(unsafe { dream_host_abi::read_bytes(request) }, unsafe {
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
pub extern "C" fn httpServerCloseUpload(id: i32) {
    crate::http_server::close_upload(id);
}

#[unsafe(no_mangle)]
pub extern "C" fn httpServerWaitAsync(future: DreamPtr, id: i32) -> i32 {
    netWsWaitAsync(future, id)
}
#[unsafe(no_mangle)]
pub extern "C" fn httpServerResult(id: i32) -> DreamPtr {
    netWsResult(id)
}
#[unsafe(no_mangle)]
pub extern "C" fn httpServerCancel(id: i32) {
    netWsCancel(id);
}
#[unsafe(no_mangle)]
pub extern "C" fn httpServerRelease(id: i32) {
    netWsRelease(id);
}
#[unsafe(no_mangle)]
pub extern "C" fn httpServerClaim(id: i32) {
    netWsClaim(id);
}

#[unsafe(no_mangle)]
pub extern "C" fn httpServerShutdown(id: i32) {
    crate::http_server::shutdown(id);
}
