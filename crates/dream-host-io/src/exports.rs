use dream_host_abi::{DreamPtr, alloc_string, read_string};

/// # Safety
/// The directory must be a readable guest string for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tempDirectoryCreate(directory: DreamPtr) -> DreamPtr {
    let directory = unsafe { read_string(directory) };
    let mut builder = tempfile::Builder::new();
    builder.prefix(".dream-");
    let temporary = if directory.is_empty() {
        builder.tempdir()
    } else {
        builder.tempdir_in(directory)
    };
    let Ok(temporary) = temporary else {
        return alloc_string("");
    };
    let Some(path) = temporary.path().to_str().map(str::to_owned) else {
        return alloc_string("");
    };
    let _ = temporary.keep();
    alloc_string(&path)
}

/// # Safety
/// Both paths must be readable guest strings for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tempReplace(source: DreamPtr, destination: DreamPtr) -> i32 {
    let source = unsafe { read_string(source) };
    let destination = unsafe { read_string(destination) };
    i32::from(std::fs::rename(source, destination).is_ok())
}

#[unsafe(no_mangle)]
pub extern "C" fn tempSupported() -> i32 {
    1
}
