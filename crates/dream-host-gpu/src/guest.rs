//! Guest byte arrays that asynchronous reads fill before resolving their future.
//!
//! The array is allocated on the guest thread when the read starts (completion threads must not
//! allocate from the guest's thread-local arena), filled under the host lock once the GPU is
//! done, and handed to the future as the owned result a synchronous host would return.

use crate::waiter;

pub struct OutBytes {
    ptr: usize,
    len: usize,
}

impl OutBytes {
    /// A zeroed array of `len` bytes; `None` when the guest cannot allocate it.
    pub fn alloc(len: usize) -> Option<Self> {
        let ptr = if waiter::has_completion_hook() {
            // Host tests run without a guest; mimic the guest array layout.
            let storage = vec![0u8; len + dream_host_abi::ARRAY_COUNT_SIZE].into_boxed_slice();
            let raw = Box::into_raw(storage) as *mut u8;
            unsafe { raw.cast::<i32>().write_unaligned(len as i32) };
            raw
        } else {
            dream_host_abi::alloc_bytes(&vec![0u8; len])
        };
        (!ptr.is_null()).then_some(Self {
            ptr: ptr as usize,
            len,
        })
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        if waiter::has_completion_hook() {
            unsafe {
                std::slice::from_raw_parts_mut(
                    (self.ptr as *mut u8).add(dream_host_abi::ARRAY_COUNT_SIZE),
                    self.len,
                )
            }
        } else {
            unsafe { dream_host_abi::bytes_mut(self.ptr as *mut u8) }
        }
    }

    /// The future value: the array pointer.
    pub fn finish(self) -> u64 {
        self.ptr as u64
    }

    /// The future value for a failed read: the same array, now empty. Its storage is freed with
    /// the array; only the visible length changes.
    pub fn fail(self) -> u64 {
        if waiter::has_completion_hook() {
            unsafe { (self.ptr as *mut u8).cast::<i32>().write_unaligned(0) };
        } else {
            unsafe { dream_host_abi::truncate_array(self.ptr as *mut u8, 0) };
        }
        self.ptr as u64
    }
}

/// Reads a guest-layout byte array produced by [`OutBytes`] in host tests.
///
/// # Safety
/// `value` must be a future value an [`OutBytes`] produced while the completion hook was set.
pub unsafe fn test_bytes(value: u64) -> Vec<u8> {
    let ptr = value as *const u8;
    let len = unsafe { ptr.cast::<i32>().read_unaligned() } as usize;
    unsafe { std::slice::from_raw_parts(ptr.add(dream_host_abi::ARRAY_COUNT_SIZE), len) }.to_vec()
}
