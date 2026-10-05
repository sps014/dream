// Guest pointers come from the validated runtime C ABI, not arbitrary Rust callers.
#![allow(clippy::missing_safety_doc)]

use dream_host_abi::*;

#[no_mangle]
pub unsafe extern "C" fn dateZoneOffsetMinutes(zone_name: DreamPtr, epoch_millis: i64) -> i32 {
    crate::tz::zone_offset_minutes(&read_string(zone_name), epoch_millis)
}

#[no_mangle]
pub extern "C" fn dateLocalZoneName() -> DreamPtr {
    alloc_string(&crate::tz::local_zone_name())
}
