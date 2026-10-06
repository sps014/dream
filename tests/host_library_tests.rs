//! Exercise real dynamic-library boundaries through retained services.

use dream_abi::host_capability::HostCapability;
use libloading::{Library, Symbol};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex};

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
            let marker = format!("dream_host_{}_abi_v2", capability.name());
            let marker: Symbol<unsafe extern "C" fn()> = library.get(marker.as_bytes()).unwrap();
            marker();
        }
        type Bind = unsafe extern "C" fn(
            unsafe extern "C" fn(i32) -> *mut u8,
            unsafe extern "C" fn(i32, i32) -> *mut u8,
            extern "C" fn(*mut u8, u64),
        );
        let bind: Symbol<Bind> = libraries[0].get(b"dream_host_bind_v2").unwrap();
        bind(string_alloc, array_new, complete);

        let unicode: Symbol<unsafe extern "C" fn(*mut u8) -> *mut u8> =
            libraries[1].get(b"unicodeToLower").unwrap();
        let crypto: Symbol<unsafe extern "C" fn(i32) -> *mut u8> =
            libraries[2].get(b"cryptoSecureRandomBytes").unwrap();
        let process: Symbol<unsafe extern "C" fn(i32) -> *mut u8> =
            libraries[3].get(b"processWait").unwrap();
        let timezone: Symbol<unsafe extern "C" fn() -> *mut u8> =
            libraries[4].get(b"dateLocalZoneName").unwrap();
        let before_strings = STRINGS.load(Ordering::Relaxed);
        let before_arrays = ARRAYS.load(Ordering::Relaxed);
        assert!(!unicode(std::ptr::null_mut()).is_null());
        let random = crypto(7);
        assert_eq!(*(random as *const i32), 7);
        assert!(!process(-1).is_null());
        assert!(!timezone().is_null());
        assert_eq!(STRINGS.load(Ordering::Relaxed), before_strings + 2);
        assert_eq!(ARRAYS.load(Ordering::Relaxed), before_arrays + 2);

        ALLOCATIONS.lock().unwrap().clear();
        drop(libraries);
    }
}
