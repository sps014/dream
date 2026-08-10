//! Helpers for reading native (host) pointers/C strings from `@c` callbacks.
//!
//! SQLite (and most C libraries) pass `char*` / `T*` that live in the **host** address space, not
//! Dream linear memory. These imports copy those values into Dream strings / `long` handles.

use std::ffi::CStr;
use std::os::raw::c_char;

use wasmtime::*;

use super::memory::write_string_to_memory;

/// Registers `dream_ffi.read_ptr` / `dream_ffi.read_cstring` on the linker.
pub fn link_ffi_helpers(linker: &mut Linker<()>) -> Result<()> {
    // `*(base as *const *const void).add(index)` — index into `char**` / pointer arrays.
    linker.func_wrap("dream_ffi", "read_ptr", |base: i64, index: i32| -> i64 {
        if base == 0 {
            return 0;
        }
        unsafe {
            let slot = (base as *const usize).wrapping_add(index as usize);
            *slot as i64
        }
    })?;

    // NUL-terminated UTF-8 `char*` → Dream `string` (empty when ptr is null).
    linker.func_wrap(
        "dream_ffi",
        "read_cstring",
        |mut caller: Caller<'_, ()>, ptr: i64| -> Result<i32> {
            if ptr == 0 {
                return write_string_to_memory(&mut caller, "");
            }
            let s = unsafe {
                CStr::from_ptr(ptr as *const c_char)
                    .to_string_lossy()
                    .into_owned()
            };
            write_string_to_memory(&mut caller, &s)
        },
    )?;

    Ok(())
}
