//! Target-neutral guest-heap views: WASM linear memory and native `dream-rt` share the same
//! `i32` pointer + length-prefixed string layout.

use dream_mir::abi;

/// Byte view of guest memory (wasmtime `SharedMemory` or a native heap snapshot).
pub trait GuestMemory {
    fn guest_bytes(&self) -> &[u8];
}

/// Reads a Dream `string` at guest pointer `ptr` from an arbitrary byte slice.
pub fn read_string_from_slice(data: &[u8], ptr: i32) -> String {
    if ptr < 0 {
        return String::new();
    }
    let base = ptr as usize;
    if base
        .checked_add(abi::LEN_PREFIX_SIZE as usize)
        .is_none_or(|e| e > data.len())
    {
        return String::new();
    }
    let len = i32::from_le_bytes([data[base], data[base + 1], data[base + 2], data[base + 3]]);
    if len < 0 {
        return String::new();
    }
    let start = base + abi::STRING_UTF8_OFFSET as usize;
    let end = start.saturating_add(len as usize).min(data.len());
    String::from_utf8_lossy(&data[start..end]).into_owned()
}

pub fn read_string_from_guest(mem: &impl GuestMemory, ptr: i32) -> String {
    read_string_from_slice(mem.guest_bytes(), ptr)
}
