//! Guest payload conversions shared by capability libraries.
//!
//! Allocation and completion go through the core C ABI so Rust static linking cannot
//! duplicate the callback state. Counts retain Dream's int ABI; references use native pointers.

pub mod window_surface;

pub use dream_abi::guest_contract::{GUEST_ABI_VERSION, GuestContext, HostStatus};

pub type DreamPtr = *mut u8;

/// A published future stays retained by the guest scheduler until foreign completion.
pub struct ForeignFuture(DreamPtr);

// Publication and the scheduler retain make transport safe; this token cannot dereference it.
unsafe impl Send for ForeignFuture {}

impl ForeignFuture {
    pub fn new(pointer: DreamPtr) -> Self {
        Self(pointer)
    }
}

unsafe extern "C" {
    fn dream_host_data(reference: DreamPtr, kind: u32) -> DreamPtr;
    fn dream_host_length(reference: DreamPtr) -> u32;
    fn dream_host_truncate(reference: DreamPtr, length: u32) -> u32;
    fn dream_host_string_alloc(length: i32) -> DreamPtr;
    fn dream_host_array_new(length: i32, element_size: i32) -> DreamPtr;
    fn dream_host_complete_foreign(future: DreamPtr, result: u64);
    fn dream_host_app_icon(length: *mut usize) -> *const u8;
}

pub fn complete_foreign_future(future: ForeignFuture, result: u64) {
    unsafe { dream_host_complete_foreign(future.0, result) }
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
/// `reference` is a readable guest string, retained for the duration of this call.
pub unsafe fn read_string(reference: DreamPtr) -> String {
    if reference.is_null() {
        return String::new();
    }
    let n = unsafe { dream_host_length(reference) } as usize;
    if n == 0 {
        return String::new();
    }
    let data = unsafe { dream_host_data(reference, dream_abi::guest_contract::GUEST_STRING) };
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(data.cast::<u16>(), n) })
}

/// # Safety
/// `reference` is a readable guest byte array retained for this call.
pub unsafe fn read_bytes(reference: DreamPtr) -> Vec<u8> {
    unsafe { read_elements(reference) }
}

/// # Safety
/// `reference` is a readable guest int array retained for this call.
pub unsafe fn read_i32s(reference: DreamPtr) -> Vec<i32> {
    unsafe { read_elements(reference) }
}

/// # Safety
/// `reference` owns a guest byte array with exclusive access for the returned borrow.
pub unsafe fn bytes_mut<'a>(reference: DreamPtr) -> &'a mut [u8] {
    let length = unsafe { dream_host_length(reference) } as usize;
    if length == 0 {
        return &mut [];
    }
    let data = unsafe { dream_host_data(reference, dream_abi::guest_contract::GUEST_ARRAY) };
    unsafe { std::slice::from_raw_parts_mut(data, length) }
}

/// # Safety
/// `reference` is a uniquely borrowed guest array. Shrinking does not drop its elements.
pub unsafe fn truncate_array(reference: DreamPtr, length: u32) -> bool {
    unsafe { dream_host_truncate(reference, length) == 0 }
}

unsafe fn read_elements<T: Copy>(reference: DreamPtr) -> Vec<T> {
    if reference.is_null() {
        return Vec::new();
    }
    let n = unsafe { dream_host_length(reference) } as usize;
    let data = unsafe { dream_host_data(reference, dream_abi::guest_contract::GUEST_ARRAY) };
    // Array payloads need not align 64-bit elements on every native target.
    (0..n)
        .map(|i| unsafe { data.cast::<T>().add(i).read_unaligned() })
        .collect()
}

pub fn alloc_string(s: &str) -> DreamPtr {
    let units: Vec<u16> = s.encode_utf16().collect();
    let Ok(length) = i32::try_from(units.len()) else {
        return std::ptr::null_mut();
    };
    let p = unsafe { dream_host_string_alloc(length) };
    if !p.is_null() && !units.is_empty() {
        let data = unsafe { dream_host_data(p, dream_abi::guest_contract::GUEST_STRING) };
        unsafe { std::ptr::copy_nonoverlapping(units.as_ptr(), data.cast::<u16>(), units.len()) };
    }
    p
}

fn alloc_elements<T: Copy>(items: &[T]) -> DreamPtr {
    let Ok(length) = i32::try_from(items.len()) else {
        return std::ptr::null_mut();
    };
    let p = unsafe { dream_host_array_new(length, std::mem::size_of::<T>() as i32) };
    if !p.is_null() && !items.is_empty() {
        let data = unsafe { dream_host_data(p, dream_abi::guest_contract::GUEST_ARRAY) };
        unsafe {
            std::ptr::copy_nonoverlapping(
                items.as_ptr().cast::<u8>(),
                data,
                std::mem::size_of_val(items),
            )
        };
    }
    p
}

pub fn alloc_bytes(items: &[u8]) -> DreamPtr {
    alloc_elements(items)
}
pub fn alloc_i64s(items: &[i64]) -> DreamPtr {
    alloc_elements(items)
}
pub fn alloc_i32s(items: &[i32]) -> DreamPtr {
    alloc_elements(items)
}
pub fn alloc_f32s(items: &[f32]) -> DreamPtr {
    alloc_elements(items)
}

pub fn alloc_string_array(items: &[String]) -> DreamPtr {
    let Ok(length) = i32::try_from(items.len()) else {
        return std::ptr::null_mut();
    };
    let p = unsafe { dream_host_array_new(length, std::mem::size_of::<DreamPtr>() as i32) };
    if !p.is_null() {
        let data = unsafe { dream_host_data(p, dream_abi::guest_contract::GUEST_ARRAY) };
        for (i, item) in items.iter().enumerate() {
            unsafe {
                data.cast::<DreamPtr>()
                    .add(i)
                    .write_unaligned(alloc_string(item))
            };
        }
    }
    p
}
