// Guest pointers come from the validated runtime C ABI, not arbitrary Rust callers.
#![allow(clippy::missing_safety_doc)]

use dream_host_abi::{DreamPtr, ForeignFuture, read_string};

#[unsafe(no_mangle)]
pub extern "C" fn appStart(shutdown_timeout_ms: i32) -> i32 {
    crate::install_watchers();
    crate::refresh_facts();
    i32::from(crate::queue::start(shutdown_timeout_ms))
}

#[unsafe(no_mangle)]
pub extern "C" fn appNextEventAsync(future: DreamPtr) -> i32 {
    crate::queue::next(ForeignFuture::new(future));
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn appEventValue() -> i32 {
    crate::queue::value()
}

#[unsafe(no_mangle)]
pub extern "C" fn appReply(value: i32) {
    crate::queue::reply(value);
}

#[unsafe(no_mangle)]
pub extern "C" fn appQuery(fact: i32) -> i32 {
    crate::facts::get(fact)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn appReport(message: DreamPtr) {
    eprintln!("{}", unsafe { read_string(message) });
}

#[unsafe(no_mangle)]
pub extern "C" fn appStop() {
    crate::queue::stop();
}
