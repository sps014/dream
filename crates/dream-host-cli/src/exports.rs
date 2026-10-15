use dream_host_abi::{DreamPtr, alloc_string, read_string};

/// # Safety
/// Request must point to a readable guest string for this call's duration.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cliParse(request: DreamPtr) -> DreamPtr {
    let request = unsafe { read_string(request) };
    alloc_string(&crate::parse::parse(&request))
}
