//! Guest heap access for native hosts. Pointers are `i32` offsets into `dream-rt` C heap.

use std::os::raw::c_char;

extern "C" {
    fn dream_heap_base() -> *mut u8;
    fn dream_heap_cap() -> i32;
    fn dream_malloc(size: i32, tag: i32) -> i32;
    fn dream_intern_utf8(bytes: *const c_char, len: i32) -> i32;
    fn dream_str_byte_size(ptr: i32) -> i32;
    fn dream_store_i32(addr: i32, value: i32);
    fn dream_load_i32(addr: i32) -> i32;
}

const TAG_ARRAY: i32 = dream_mir::abi::TAG_ARRAY;
const STRING_UTF8: i32 = dream_mir::abi::STRING_UTF8_OFFSET as i32;

pub fn heap_cap() -> usize {
    unsafe { dream_heap_cap().max(0) as usize }
}

pub fn in_range(ptr: i32, n: usize) -> bool {
    if ptr < 0 {
        return false;
    }
    (ptr as usize)
        .checked_add(n)
        .map(|end| end <= heap_cap())
        .unwrap_or(false)
}

pub fn copy_out(ptr: i32, n: usize) -> Option<Vec<u8>> {
    if n == 0 {
        return Some(Vec::new());
    }
    if !in_range(ptr, n) {
        return None;
    }
    unsafe {
        Some(std::slice::from_raw_parts(dream_heap_base().add(ptr as usize), n).to_vec())
    }
}

pub fn copy_in(ptr: i32, bytes: &[u8]) {
    if bytes.is_empty() || !in_range(ptr, bytes.len()) {
        return;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            dream_heap_base().add(ptr as usize),
            bytes.len(),
        );
    }
}

pub fn read_string(ptr: i32) -> String {
    unsafe {
        let n = dream_str_byte_size(ptr);
        if n <= 0 || !in_range(ptr + STRING_UTF8, n as usize) {
            return String::new();
        }
        let base = dream_heap_base();
        let sl = std::slice::from_raw_parts(base.add((ptr + STRING_UTF8) as usize), n as usize);
        String::from_utf8_lossy(sl).into_owned()
    }
}

pub fn intern(s: &str) -> i32 {
    unsafe { dream_intern_utf8(s.as_ptr() as *const c_char, s.len() as i32) }
}

pub fn read_bytes(ptr: i32) -> Vec<u8> {
    if ptr <= 0 {
        return Vec::new();
    }
    unsafe {
        let n = dream_load_i32(ptr);
        if n <= 0 || !in_range(ptr + 4, n as usize) {
            return Vec::new();
        }
        let base = dream_heap_base();
        std::slice::from_raw_parts(base.add((ptr + 4) as usize), n as usize).to_vec()
    }
}

pub fn write_bytes(bytes: &[u8]) -> i32 {
    let n = bytes.len() as i32;
    unsafe {
        let p = dream_malloc(4 + n, TAG_ARRAY);
        dream_store_i32(p, n);
        if n > 0 {
            let base = dream_heap_base();
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), base.add((p + 4) as usize), bytes.len());
        }
        p
    }
}

pub fn fill_bytes(ptr: i32, bytes: &[u8]) {
    if ptr <= 0 {
        return;
    }
    unsafe {
        let n = dream_load_i32(ptr).max(0) as usize;
        let take = n.min(bytes.len());
        if take == 0 || !in_range(ptr + 4, take) {
            return;
        }
        let base = dream_heap_base();
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), base.add((ptr + 4) as usize), take);
    }
}

pub fn read_i32_array(ptr: i32) -> Vec<i32> {
    if ptr <= 0 {
        return Vec::new();
    }
    unsafe {
        let n = dream_load_i32(ptr);
        if n <= 0 {
            return Vec::new();
        }
        let base = dream_heap_base();
        let mut out = Vec::with_capacity(n as usize);
        for i in 0..n {
            let off = (ptr + 4 + i * 4) as usize;
            let sl = std::slice::from_raw_parts(base.add(off), 4);
            out.push(i32::from_le_bytes([sl[0], sl[1], sl[2], sl[3]]));
        }
        out
    }
}

pub fn write_i32_array(values: &[i32]) -> i32 {
    let n = values.len() as i32;
    unsafe {
        let p = dream_malloc(4 + n * 4, TAG_ARRAY);
        dream_store_i32(p, n);
        let base = dream_heap_base();
        for (i, v) in values.iter().enumerate() {
            let b = v.to_le_bytes();
            std::ptr::copy_nonoverlapping(b.as_ptr(), base.add((p + 4 + (i as i32) * 4) as usize), 4);
        }
        p
    }
}

pub fn write_string_array(items: &[String]) -> i32 {
    let count = items.len() as i32;
    unsafe {
        let p = dream_malloc(4 + count * 4, TAG_ARRAY);
        dream_store_i32(p, count);
        for (i, item) in items.iter().enumerate() {
            let sp = intern(item);
            dream_store_i32(p + 4 + (i as i32) * 4, sp);
        }
        p
    }
}
