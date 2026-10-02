//! Guest payload conversions shared by capability libraries.
//!
//! Allocation and completion go through the core C ABI so Rust static linking cannot
//! duplicate the callback state. Counts retain Dream's int ABI; addresses use usize.

extern "C" {
    fn dream_host_string_alloc(length: i32) -> usize;
    fn dream_host_array_new(length: i32, element_size: i32) -> usize;
    fn dream_host_complete_foreign(future: usize, result: usize);
    fn dream_host_app_icon(length: *mut usize) -> *const u8;
}

pub fn complete_foreign_future(future: usize, result: usize) {
    unsafe { dream_host_complete_foreign(future, result) }
}

pub fn app_icon_png() -> Option<&'static [u8]> {
    let mut length = 0;
    let bytes = unsafe { dream_host_app_icon(&mut length) };
    if bytes.is_null() {
        None
    } else {
        // The core stores only process-lifetime constants from a guest constructor.
        Some(unsafe { std::slice::from_raw_parts(bytes, length) })
    }
}

/// # Safety
/// `p` must be zero or a valid guest string payload. Its header and UTF-16
/// storage (including a slice's backing storage) must remain readable during this call.
pub unsafe fn read_string(p: usize) -> String {
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

/// # Safety
/// `p` must be zero or a valid guest byte-array payload with readable header and elements.
pub unsafe fn read_bytes(p: usize) -> Vec<u8> {
    if p == 0 {
        return Vec::new();
    }
    let n = *(p as *const i32);
    if n <= 0 {
        return Vec::new();
    }
    std::slice::from_raw_parts((p as *const u8).add(4), n as usize).to_vec()
}

/// # Safety
/// `p` must be zero or a valid guest int-array payload with readable, i32-aligned elements.
pub unsafe fn read_i32s(p: usize) -> Vec<i32> {
    if p == 0 {
        return Vec::new();
    }
    let n = *(p as *const i32);
    if n <= 0 {
        return Vec::new();
    }
    std::slice::from_raw_parts((p as *const u8).add(4).cast::<i32>(), n as usize).to_vec()
}

pub fn alloc_string(s: &str) -> usize {
    let units: Vec<u16> = s.encode_utf16().collect();

    unsafe {
        let p = dream_host_string_alloc(units.len() as i32);
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

pub fn alloc_bytes(bytes: &[u8]) -> usize {
    unsafe {
        let p = dream_host_array_new(bytes.len() as i32, 1);
        if p == 0 {
            return 0;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), (p as *mut u8).add(4), bytes.len());
        p
    }
}

pub fn alloc_i64s(xs: &[i64]) -> usize {
    unsafe {
        let p = dream_host_array_new(xs.len() as i32, 8);
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

pub fn alloc_i32s(xs: &[i32]) -> usize {
    unsafe {
        let p = dream_host_array_new(xs.len() as i32, 4);
        if p == 0 {
            return 0;
        }
        std::ptr::copy_nonoverlapping(xs.as_ptr(), (p as *mut u8).add(4).cast::<i32>(), xs.len());
        p
    }
}

pub fn alloc_string_array(items: &[String]) -> usize {
    unsafe {
        let p = dream_host_array_new(items.len() as i32, 8);
        if p == 0 {
            return 0;
        }
        let slots = (p as *mut u8).add(4).cast::<usize>();
        for (i, item) in items.iter().enumerate() {
            slots.add(i).write_unaligned(alloc_string(item));
        }
        p
    }
}
