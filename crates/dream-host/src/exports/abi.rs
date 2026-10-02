//! Shared guest allocation/binding ABI owned by `dream-host`.
//!
//! Pointer arguments are guest heap addresses from the C runtime, not Rust references.

use std::sync::Mutex;

type AllocFn = unsafe extern "C" fn(i32) -> usize;
type ArrayNewFn = unsafe extern "C" fn(i32, i32) -> usize;
type CompleteForeignFn = unsafe extern "C" fn(usize, usize);

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
pub extern "C" fn dream_host_bind(
    string_alloc: AllocFn,
    array_new: ArrayNewFn,
    complete_foreign: CompleteForeignFn,
) {
    let mut g = GUEST.lock().expect("guest alloc");
    g.string_alloc = Some(string_alloc);
    g.array_new = Some(array_new);
    g.complete_foreign = Some(complete_foreign);
}

/// Completes an `@async_host` future from a foreign thread and wakes the guest run loop.
#[cfg(feature = "net")]
pub(super) fn complete_foreign_future(future: usize, result: usize) {
    let complete = GUEST.lock().ok().and_then(|g| g.complete_foreign);
    if let Some(complete) = complete {
        unsafe {
            complete(future, result);
        }
    }
}

pub(super) unsafe fn read_string(p: usize) -> String {
    if p == 0 {
        return String::new();
    }
    let n = *(p as *const i32);
    if n <= 0 {
        return String::new();
    }
    const DREAM_STR_SLICE: i32 = dream_mir::abi::DREAM_STR_SLICE;
    let pad = *((p as *const i32).add(1));
    let units = if pad == DREAM_STR_SLICE {
        let d = std::ptr::read(
            (p as *const u8)
                .add(dream_mir::abi::STRING_HEADER_SIZE as usize + std::mem::size_of::<usize>())
                .cast::<*const u16>(),
        );
        std::slice::from_raw_parts(d, n as usize)
    } else {
        std::slice::from_raw_parts(
            (p as *const u8)
                .add(dream_mir::abi::STRING_UNITS_OFFSET as usize)
                .cast::<u16>(),
            n as usize,
        )
    };
    String::from_utf16_lossy(units)
}

pub(super) unsafe fn read_bytes(p: usize) -> Vec<u8> {
    if p == 0 {
        return Vec::new();
    }
    let n = *(p as *const i32);
    if n <= 0 {
        return Vec::new();
    }
    std::slice::from_raw_parts((p as *const u8).add(4), n as usize).to_vec()
}

#[cfg(feature = "gpu")]
pub(super) unsafe fn read_i32s(p: usize) -> Vec<i32> {
    if p == 0 {
        return Vec::new();
    }
    let n = *(p as *const i32);
    if n <= 0 {
        return Vec::new();
    }
    std::slice::from_raw_parts((p as *const u8).add(4).cast::<i32>(), n as usize).to_vec()
}

pub(super) fn alloc_string(s: &str) -> usize {
    let units: Vec<u16> = s.encode_utf16().collect();
    let alloc = GUEST.lock().ok().and_then(|g| g.string_alloc);
    let Some(alloc) = alloc else {
        return 0;
    };
    unsafe {
        let p = alloc(units.len() as i32);
        if p == 0 {
            return 0;
        }
        let dst = (p as *mut u8)
            .add(dream_mir::abi::STRING_UNITS_OFFSET as usize)
            .cast::<u16>();
        std::ptr::copy_nonoverlapping(units.as_ptr(), dst, units.len());
        p
    }
}

pub(super) fn alloc_bytes(bytes: &[u8]) -> usize {
    let alloc = GUEST.lock().ok().and_then(|g| g.array_new);
    let Some(alloc) = alloc else {
        return 0;
    };
    unsafe {
        let p = alloc(bytes.len() as i32, 1);
        if p == 0 {
            return 0;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), (p as *mut u8).add(4), bytes.len());
        p
    }
}

#[cfg(feature = "gpu")]
pub(super) fn alloc_i64s(xs: &[i64]) -> usize {
    let alloc = GUEST.lock().ok().and_then(|g| g.array_new);
    let Some(alloc) = alloc else {
        return 0;
    };
    unsafe {
        let p = alloc(xs.len() as i32, 8);
        if p == 0 {
            return 0;
        }
        std::ptr::copy_nonoverlapping(
            xs.as_ptr().cast::<u8>(),
            (p as *mut u8).add(4),
            xs.len() * 8,
        );
        p
    }
}

#[cfg(feature = "gpu")]
pub(super) fn alloc_i32s(xs: &[i32]) -> usize {
    let alloc = GUEST.lock().ok().and_then(|g| g.array_new);
    let Some(alloc) = alloc else {
        return 0;
    };
    unsafe {
        let p = alloc(xs.len() as i32, 4);
        if p == 0 {
            return 0;
        }
        std::ptr::copy_nonoverlapping(xs.as_ptr(), (p as *mut u8).add(4).cast::<i32>(), xs.len());
        p
    }
}

#[cfg(feature = "core")]
pub(super) fn alloc_string_array(items: &[String]) -> usize {
    let alloc = GUEST.lock().ok().and_then(|g| g.array_new);
    let Some(alloc) = alloc else {
        return 0;
    };
    unsafe {
        let p = alloc(items.len() as i32, 8);
        if p == 0 {
            return 0;
        }
        let slots = (p as *mut u8).add(4).cast::<usize>();
        for (i, item) in items.iter().enumerate() {
            slots.add(i).write(alloc_string(item));
        }
        p
    }
}
