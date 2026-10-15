use dream_host_abi::{DreamPtr, alloc_string, read_string};
/// # Safety
/// Request must be a readable guest string for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dataCodec(request: DreamPtr) -> DreamPtr {
    let request = unsafe { read_string(request) };
    alloc_string(&crate::codecs::request(&request))
}
