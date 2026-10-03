//! The single process-wide guest callback table, owned only by the core cdylib.

use dream_host_abi::DreamPtr;
use std::sync::Mutex;

type AllocFn = unsafe extern "C" fn(i32) -> DreamPtr;
type ArrayNewFn = unsafe extern "C" fn(i32, i32) -> DreamPtr;
type CompleteForeignFn = unsafe extern "C" fn(DreamPtr, u64);

struct GuestAlloc {
    string_alloc: Option<AllocFn>,
    array_new: Option<ArrayNewFn>,
    complete_foreign: Option<CompleteForeignFn>,
}

static GUEST: Mutex<GuestAlloc> = Mutex::new(GuestAlloc {
    string_alloc: None,
    array_new: None,
    complete_foreign: None,
});

#[no_mangle]
pub extern "C" fn dream_host_bind_v2(
    string_alloc: AllocFn,
    array_new: ArrayNewFn,
    complete_foreign: CompleteForeignFn,
) {
    *GUEST.lock().expect("guest allocation table") = GuestAlloc {
        string_alloc: Some(string_alloc),
        array_new: Some(array_new),
        complete_foreign: Some(complete_foreign),
    };
}

#[no_mangle]
pub extern "C" fn dream_host_string_alloc(length: i32) -> DreamPtr {
    let alloc = GUEST.lock().ok().and_then(|g| g.string_alloc);
    alloc.map_or(std::ptr::null_mut(), |alloc| unsafe { alloc(length) })
}

#[no_mangle]
pub extern "C" fn dream_host_array_new(length: i32, element_size: i32) -> DreamPtr {
    let alloc = GUEST.lock().ok().and_then(|g| g.array_new);
    alloc.map_or(std::ptr::null_mut(), |alloc| unsafe {
        alloc(length, element_size)
    })
}

#[no_mangle]
pub extern "C" fn dream_host_complete_foreign(future: DreamPtr, result: u64) {
    let complete = GUEST.lock().ok().and_then(|g| g.complete_foreign);
    if let Some(complete) = complete {
        unsafe { complete(future, result) }
    }
}
