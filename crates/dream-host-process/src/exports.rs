use dream_host_abi::{DreamPtr, alloc_bytes, read_bytes, read_string};

/// # Safety
/// Request and bytes must be readable guest string and byte-array payloads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn processStart(request: DreamPtr, bytes: DreamPtr) -> i32 {
    let (request, bytes) = unsafe { (read_string(request), read_bytes(bytes)) };
    crate::service::start(request, bytes)
}
#[unsafe(no_mangle)]
pub extern "C" fn processOperationResult(id: i32) -> DreamPtr {
    alloc_bytes(&crate::operations::OPERATIONS.result(id))
}
#[unsafe(no_mangle)]
pub extern "C" fn processOperationCancel(id: i32) {
    crate::operations::OPERATIONS.cancel(id);
}
#[unsafe(no_mangle)]
pub extern "C" fn processOperationRelease(id: i32) {
    crate::operations::OPERATIONS.release(id);
}
#[unsafe(no_mangle)]
pub extern "C" fn processOperationClaim(id: i32) {
    crate::operations::OPERATIONS.claim(id);
}
#[unsafe(no_mangle)]
pub extern "C" fn processClose(id: i32) {
    crate::service::close(id);
}
#[unsafe(no_mangle)]
pub extern "C" fn processOperationWaitAsync(future: DreamPtr, id: i32) -> i32 {
    if !crate::operations::OPERATIONS.wait(id, dream_host_abi::ForeignFuture::new(future)) {
        dream_host_abi::complete_foreign_future(dream_host_abi::ForeignFuture::new(future), 0);
    }
    1
}
