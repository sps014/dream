//! The single process-wide guest callback table, owned only by the core cdylib.

use dream_host_abi::{DreamPtr, GUEST_ABI_VERSION, GuestContext, HostStatus};
use std::sync::Mutex;

static GUEST: Mutex<Option<GuestContext>> = Mutex::new(None);

/// # Safety
/// `context` points to a readable table. Its callbacks remain valid while hosts are loaded.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dream_host_bind_v3(context: *const GuestContext) -> u32 {
    if context.is_null() {
        return HostStatus::Invalid as u32;
    }
    // Only the fixed prefix is valid until the version and size have been checked.
    let version = unsafe { context.cast::<u32>().read_unaligned() };
    let size = unsafe { context.cast::<u32>().add(1).read_unaligned() };
    if version != GUEST_ABI_VERSION || size as usize != std::mem::size_of::<GuestContext>() {
        return HostStatus::Version as u32;
    }
    let context = unsafe { context.read_unaligned() };
    if context.string_alloc.is_none()
        || context.array_new.is_none()
        || context.data.is_none()
        || context.length.is_none()
        || context.truncate.is_none()
        || context.complete.is_none()
        || context.status.is_none()
        || context.retain.is_none()
        || context.release.is_none()
        || context.set_park.is_none()
    {
        return HostStatus::Invalid as u32;
    }
    match GUEST.lock() {
        Ok(mut guest) => {
            *guest = Some(context);
            0
        }
        Err(_) => HostStatus::Failed as u32,
    }
}

fn guest() -> Option<GuestContext> {
    GUEST.lock().ok().and_then(|guest| *guest)
}

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_string_alloc(length: i32) -> DreamPtr {
    if length < 0 {
        return std::ptr::null_mut();
    }
    guest()
        .and_then(|g| g.string_alloc)
        .map_or(std::ptr::null_mut(), |f| unsafe { f(length) })
}

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_array_new(length: i32, element_size: i32) -> DreamPtr {
    if length < 0 || element_size < 1 {
        return std::ptr::null_mut();
    }
    guest()
        .and_then(|g| g.array_new)
        .map_or(std::ptr::null_mut(), |f| unsafe { f(length, element_size) })
}

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_data(reference: DreamPtr, kind: u32) -> DreamPtr {
    guest()
        .and_then(|g| g.data)
        .map_or(std::ptr::null_mut(), |f| unsafe { f(reference, kind) })
}

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_length(reference: DreamPtr) -> u32 {
    guest()
        .and_then(|g| g.length)
        .map_or(0, |f| unsafe { f(reference) })
}

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_truncate(reference: DreamPtr, length: u32) -> u32 {
    guest()
        .and_then(|g| g.truncate)
        .map_or(HostStatus::Closed as u32, |f| unsafe {
            f(reference, length)
        })
}

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_complete_foreign(future: DreamPtr, result: u64) {
    if let Some(complete) = guest().and_then(|g| g.complete) {
        unsafe { complete(future, result) }
    }
}

/// Installs (or, with null hooks, removes) the calling thread's scheduler park hooks.
#[unsafe(no_mangle)]
pub extern "C" fn dream_host_set_park(
    park: Option<unsafe extern "C" fn(i64, *mut u8)>,
    wake: Option<unsafe extern "C" fn(*mut u8)>,
    context: *mut u8,
) {
    if let Some(set_park) = guest().and_then(|g| g.set_park) {
        unsafe { set_park(park, wake, context) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_incompatible_context_before_reading_callbacks() {
        let prefix = [GUEST_ABI_VERSION + 1, 8];
        assert_eq!(
            unsafe { dream_host_bind_v3(prefix.as_ptr().cast()) },
            HostStatus::Version as u32
        );
        let prefix = [GUEST_ABI_VERSION, 8];
        assert_eq!(
            unsafe { dream_host_bind_v3(prefix.as_ptr().cast()) },
            HostStatus::Version as u32
        );
        assert_eq!(
            unsafe { dream_host_bind_v3(std::ptr::null()) },
            HostStatus::Invalid as u32
        );
    }
}
