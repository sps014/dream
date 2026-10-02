//! Exercise real dynamic-library boundaries without creating windows or contacting a server.

use dream_abi::host_capability::HostCapability;
use libloading::{Library, Symbol};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::Duration;

static ALLOCATIONS: Mutex<Vec<Box<[u64]>>> = Mutex::new(Vec::new());
static STRINGS: AtomicUsize = AtomicUsize::new(0);
static ARRAYS: AtomicUsize = AtomicUsize::new(0);
static COMPLETION: Mutex<Option<mpsc::Sender<(usize, usize)>>> = Mutex::new(None);

unsafe extern "C" fn string_alloc(length: i32) -> usize {
    STRINGS.fetch_add(1, Ordering::Relaxed);
    allocate(length, 2, 8)
}

unsafe extern "C" fn array_new(length: i32, element_size: i32) -> usize {
    ARRAYS.fetch_add(1, Ordering::Relaxed);
    allocate(length, element_size, 4)
}

fn allocate(length: i32, element_size: i32, header_size: usize) -> usize {
    assert!(length >= 0 && element_size > 0);
    let size = header_size + length as usize * element_size as usize;
    let mut storage = vec![0_u64; size.div_ceil(8)].into_boxed_slice();
    let pointer = storage.as_mut_ptr() as usize;
    unsafe { (pointer as *mut i32).write(length) };
    ALLOCATIONS.lock().unwrap().push(storage);
    pointer
}

extern "C" fn complete(future: usize, result: usize) {
    COMPLETION
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .send((future, result))
        .unwrap();
}

#[test]
fn capabilities_share_core_binding_allocation_completion_and_icon() {
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
            .map(|capability| Library::new(directory.join(capability.library_name())).unwrap())
            .collect();
        type Bind = unsafe extern "C" fn(
            unsafe extern "C" fn(i32) -> usize,
            unsafe extern "C" fn(i32, i32) -> usize,
            extern "C" fn(usize, usize),
        );
        let bind: Symbol<Bind> = libraries[0].get(b"dream_host_bind").unwrap();
        bind(string_alloc, array_new, complete);

        let gpu_error: Symbol<unsafe extern "C" fn() -> usize> =
            libraries[2].get(b"gpuLastError").unwrap();
        let string = gpu_error();
        assert_ne!(string, 0);
        assert_eq!(*(string as *const i32), 0);

        let shell_open: Symbol<unsafe extern "C" fn(usize) -> usize> =
            libraries[3].get(b"shellOpen").unwrap();
        let bytes = shell_open(0);
        assert_ne!(bytes, 0);
        let length = *(bytes as *const i32) as usize;
        assert_eq!(
            std::slice::from_raw_parts((bytes as *const u8).add(4), length),
            b"Shell.open: empty target"
        );

        let (sender, receiver) = mpsc::channel();
        *COMPLETION.lock().unwrap() = Some(sender);
        let request: Symbol<
            unsafe extern "C" fn(usize, usize, usize, usize, usize, i32, i32) -> i32,
        > = libraries[1].get(b"httpRequestAsync").unwrap();
        assert_eq!(request(42, 0, 0, 0, 0, 100, 0), 1);
        let (future, result) = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(future, 42);
        assert_ne!(result, 0);
        assert!(STRINGS.load(Ordering::Relaxed) >= 1);
        assert!(ARRAYS.load(Ordering::Relaxed) >= 2);

        let set_icon: Symbol<unsafe extern "C" fn(*const u8, i32)> =
            libraries[0].get(b"dream_set_app_icon").unwrap();
        static ICON: &[u8] = b"shared icon";
        set_icon(ICON.as_ptr(), ICON.len() as i32);
        // GetProcAddress does not search dependencies; cross-capability allocation
        // above still verifies the Windows libraries' binding to this core instance.
        let symbol_libraries = if cfg!(windows) {
            &libraries[..1]
        } else {
            &libraries[..]
        };
        for library in symbol_libraries {
            let icon: Symbol<unsafe extern "C" fn(*mut usize) -> *const u8> =
                library.get(b"dream_host_app_icon").unwrap();
            let mut length = 0;
            let pointer = icon(&mut length);
            assert_eq!(pointer, ICON.as_ptr());
            assert_eq!(length, ICON.len());
        }
        *COMPLETION.lock().unwrap() = None;
        ALLOCATIONS.lock().unwrap().clear();
        // Host worker/TLS destructors may still execute after completion. As in
        // a compiled program, the loaded runtime must outlive those threads.
        std::mem::forget(libraries);
    }
}
