// Guest pointers come from the validated runtime C ABI, not arbitrary Rust callers.
#![allow(clippy::missing_safety_doc)]

use dream_host_abi::*;

#[no_mangle]
pub unsafe extern "C" fn processRun(
    command: DreamPtr,
    joined_args: DreamPtr,
    cwd: DreamPtr,
) -> DreamPtr {
    alloc_bytes(&crate::process_host::process_run(
        &read_string(command),
        &read_string(joined_args),
        &read_string(cwd),
    ))
}

#[no_mangle]
pub unsafe extern "C" fn processSpawn(
    command: DreamPtr,
    joined_args: DreamPtr,
    cwd: DreamPtr,
) -> DreamPtr {
    alloc_bytes(&crate::process_host::process_spawn(
        &read_string(command),
        &read_string(joined_args),
        &read_string(cwd),
    ))
}

#[no_mangle]
pub unsafe extern "C" fn processWriteStdin(handle: i32, data: DreamPtr) -> i32 {
    crate::process_host::process_write_stdin(handle, &read_bytes(data))
}

#[no_mangle]
pub extern "C" fn processReadStream(handle: i32, stream: i32, max_bytes: i32) -> DreamPtr {
    alloc_bytes(&crate::process_host::process_read_stream(
        handle, stream, max_bytes,
    ))
}

#[no_mangle]
pub extern "C" fn processReadStreamLine(handle: i32, stream: i32) -> DreamPtr {
    alloc_bytes(&crate::process_host::process_read_stream_line(
        handle, stream,
    ))
}

#[no_mangle]
pub extern "C" fn processWait(handle: i32) -> DreamPtr {
    alloc_bytes(&crate::process_host::process_wait(handle))
}

#[no_mangle]
pub extern "C" fn processKill(handle: i32) -> i32 {
    crate::process_host::process_kill(handle)
}
