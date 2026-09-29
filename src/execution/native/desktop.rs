//! C ABI for `system.desktop` host functions (`-ldream`).

#![allow(clippy::missing_safety_doc)]

use super::abi::{alloc_bytes, read_bytes, read_string};
use crate::execution::host::desktop::wire::{self, ClipKind, DialogKind, DialogRequest};
use crate::execution::host::desktop::{clipboard, dialog, shell};

#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn desktopDialogStart(
    kind: i32,
    title: usize,
    text: usize,
    directory: usize,
    file_name: usize,
    filters: usize,
    level: i32,
    buttons: i32,
    parent: i32,
) -> i32 {
    let Some(kind) = DialogKind::from_code(kind) else {
        return 0;
    };
    let req = DialogRequest {
        title: read_string(title),
        text: read_string(text),
        directory: read_string(directory),
        file_name: read_string(file_name),
        filters: wire::parse_filters(&read_string(filters)),
        level,
        buttons,
        parent,
    };
    dialog::start(kind, &req)
}

#[no_mangle]
pub extern "C" fn desktopDialogPoll(handle: i32) -> usize {
    alloc_bytes(&wire::encode_outcome(dialog::poll(handle).as_ref()))
}

#[no_mangle]
pub unsafe extern "C" fn clipboardGet(kind: i32, format: usize) -> usize {
    let value = ClipKind::from_code(kind).and_then(|k| clipboard::get(k, &read_string(format)));
    alloc_bytes(&wire::encode_clip(value))
}

#[no_mangle]
pub unsafe extern "C" fn clipboardSet(kind: i32, format: usize, data: usize) -> i32 {
    let Some(kind) = ClipKind::from_code(kind) else {
        return 1;
    };
    match clipboard::set(kind, &read_string(format), &read_bytes(data)) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Dream Clipboard: {e}");
            1
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn clipboardHas(format: usize) -> i32 {
    i32::from(clipboard::has(&read_string(format)))
}

#[no_mangle]
pub extern "C" fn clipboardClear() {
    if let Err(e) = clipboard::clear() {
        eprintln!("Dream Clipboard.clear: {e}");
    }
}

/// Empty on success, otherwise the error message.
#[no_mangle]
pub unsafe extern "C" fn shellOpen(target: usize) -> usize {
    let msg = shell::open(&read_string(target)).err().unwrap_or_default();
    alloc_bytes(msg.as_bytes())
}
