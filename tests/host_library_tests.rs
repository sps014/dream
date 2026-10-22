//! Exercise real dynamic-library boundaries through retained services.

use dream_abi::host_capability::HostCapability;
use libloading::{Library, Symbol};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, mpsc};

static ALLOCATIONS: Mutex<Vec<Box<[u64]>>> = Mutex::new(Vec::new());
static STRINGS: AtomicUsize = AtomicUsize::new(0);
static ARRAYS: AtomicUsize = AtomicUsize::new(0);
static COMPLETION: Mutex<Option<mpsc::Sender<(usize, usize)>>> = Mutex::new(None);

unsafe extern "C" fn string_alloc(length: i32) -> *mut u8 {
    STRINGS.fetch_add(1, Ordering::Relaxed);
    allocate(length, 2, 8)
}

unsafe extern "C" fn array_new(length: i32, element_size: i32) -> *mut u8 {
    ARRAYS.fetch_add(1, Ordering::Relaxed);
    allocate(length, element_size, 4)
}

fn allocate(length: i32, element_size: i32, header_size: usize) -> *mut u8 {
    assert!(length >= 0 && element_size > 0);
    let size = header_size + length as usize * element_size as usize;
    let mut storage = vec![0_u64; size.div_ceil(8)].into_boxed_slice();
    let pointer = storage.as_mut_ptr().cast::<u8>();
    unsafe { (pointer as *mut i32).write(length) };
    ALLOCATIONS.lock().unwrap().push(storage);
    pointer
}

extern "C" fn complete(future: *mut u8, result: u64) {
    COMPLETION
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .send((future as usize, result as usize))
        .unwrap();
}

unsafe extern "C" fn guest_data(reference: *mut u8, kind: u32) -> *mut u8 {
    if reference.is_null() {
        return reference;
    }
    unsafe { reference.add(if kind == 1 { 8 } else { 4 }) }
}
unsafe extern "C" fn guest_length(reference: *mut u8) -> u32 {
    if reference.is_null() {
        return 0;
    }
    unsafe { reference.cast::<u32>().read_unaligned() }
}
unsafe extern "C" fn guest_status(_: *mut u8) -> i32 {
    0
}
unsafe extern "C" fn guest_truncate(_: *mut u8, _: u32) -> u32 {
    0
}
unsafe extern "C" fn guest_pin(_: *mut u8) {}

unsafe extern "C" fn set_park(
    _: Option<unsafe extern "C" fn(i64, *mut u8)>,
    _: Option<unsafe extern "C" fn(*mut u8)>,
    _: *mut u8,
) {
}

#[test]
fn capabilities_share_core_binding_and_allocation() {
    let directory = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    unsafe {
        let libraries: Vec<_> = HostCapability::ALL
            .iter()
            .copied()
            .map(|capability| {
                Library::new(
                    directory.join(capability.library_name(&dream_abi::target::TargetSpec::host())),
                )
                .unwrap_or_else(|e| {
                    panic!("{e}; `cargo test` does not build the host libraries, run `cargo build --workspace` first")
                })
            })
            .collect();
        for (capability, library) in HostCapability::ALL.iter().zip(&libraries) {
            let marker = capability.abi_marker();
            let marker: Symbol<unsafe extern "C" fn()> = library.get(marker.as_bytes()).unwrap();
            marker();
        }
        use dream_abi::guest_contract::{GUEST_ABI_VERSION, GuestContext};
        type Bind = unsafe extern "C" fn(*const GuestContext) -> u32;
        let bind: Symbol<Bind> = libraries[0].get(b"dream_host_bind_v3").unwrap();
        let context = GuestContext {
            version: GUEST_ABI_VERSION,
            size: std::mem::size_of::<GuestContext>() as u32,
            string_alloc: Some(string_alloc),
            array_new: Some(array_new),
            data: Some(guest_data),
            length: Some(guest_length),
            complete: Some(complete),
            truncate: Some(guest_truncate),
            status: Some(guest_status),
            retain: Some(guest_pin),
            release: Some(guest_pin),
            set_park: Some(set_park),
        };
        assert_eq!(bind(&context), 0);

        let crypto: Symbol<unsafe extern "C" fn(i32) -> *mut u8> =
            libraries[1].get(b"cryptoSecureRandomBytes").unwrap();
        let process: Symbol<unsafe extern "C" fn(i32) -> *mut u8> =
            libraries[2].get(b"processOperationResult").unwrap();
        let timezone: Symbol<unsafe extern "C" fn() -> *mut u8> =
            libraries[3].get(b"dateLocalZoneName").unwrap();
        let before_strings = STRINGS.load(Ordering::Relaxed);
        let before_arrays = ARRAYS.load(Ordering::Relaxed);
        let random = crypto(7);
        assert_eq!(*(random as *const i32), 7);
        assert!(!process(-1).is_null());
        assert!(!timezone().is_null());
        assert_eq!(STRINGS.load(Ordering::Relaxed), before_strings + 1);
        assert_eq!(ARRAYS.load(Ordering::Relaxed), before_arrays + 2);

        ALLOCATIONS.lock().unwrap().clear();
        drop(libraries);
    }
}
