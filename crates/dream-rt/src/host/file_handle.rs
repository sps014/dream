use crate::guest;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use indexmap::IndexMap;

const OPEN_ENOENT: i32 = -1;
const OPEN_EACCES: i32 = -2;
const OPEN_EINVAL: i32 = -3;
const OPEN_EIO: i32 = -4;

fn handles() -> &'static Mutex<IndexMap<u32, File>> {
    static H: OnceLock<Mutex<IndexMap<u32, File>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(IndexMap::new()))
}

static NEXT: AtomicU32 = AtomicU32::new(1);

fn map_mode(mode: &str) -> Option<OpenOptions> {
    let mut opts = OpenOptions::new();
    match mode {
        "r" => {
            opts.read(true);
        }
        "w" => {
            opts.write(true).create(true).truncate(true);
        }
        "a" => {
            opts.append(true).create(true);
        }
        "r+" => {
            opts.read(true).write(true);
        }
        "w+" => {
            opts.read(true).write(true).create(true).truncate(true);
        }
        "a+" => {
            opts.read(true).append(true).create(true);
        }
        _ => return None,
    }
    Some(opts)
}

fn map_err(e: std::io::Error) -> i32 {
    match e.kind() {
        std::io::ErrorKind::NotFound => OPEN_ENOENT,
        std::io::ErrorKind::PermissionDenied => OPEN_EACCES,
        _ => OPEN_EIO,
    }
}

#[no_mangle]
pub extern "C" fn dream_file_open(path: i32, mode: i32) -> i32 {
    let path = guest::read_string(path);
    let mode = guest::read_string(mode);
    let Some(opts) = map_mode(&mode) else {
        return OPEN_EINVAL;
    };
    match opts.open(&path) {
        Ok(file) => {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            handles().lock().unwrap_or_else(|e| e.into_inner()).insert(id, file);
            id as i32
        }
        Err(e) => map_err(e),
    }
}

#[no_mangle]
pub extern "C" fn dream_file_handle_read(id: i32, n: i32) -> i32 {
    let count = n.max(0) as usize;
    let mut buf = vec![0u8; count];
    let mut table = handles().lock().unwrap_or_else(|e| e.into_inner());
    let read_len = table
        .get_mut(&(id as u32))
        .and_then(|f| f.read(&mut buf).ok())
        .unwrap_or(0);
    guest::write_bytes(&buf[..read_len])
}

#[no_mangle]
pub extern "C" fn dream_file_handle_write(id: i32, data: i32) -> i64 {
    let bytes = guest::read_bytes(data);
    let mut table = handles().lock().unwrap_or_else(|e| e.into_inner());
    let written = table
        .get_mut(&(id as u32))
        .and_then(|f| f.write(&bytes).ok())
        .unwrap_or(0);
    if written == 0 && !bytes.is_empty() {
        -1
    } else {
        written as i64
    }
}

#[no_mangle]
pub extern "C" fn dream_file_handle_seek(id: i32, pos: i64) -> i32 {
    let mut table = handles().lock().unwrap_or_else(|e| e.into_inner());
    let ok = table
        .get_mut(&(id as u32))
        .map(|f| f.seek(SeekFrom::Start(pos as u64)).is_ok())
        .unwrap_or(false);
    if ok {
        0
    } else {
        -1
    }
}

#[no_mangle]
pub extern "C" fn dream_file_handle_close(id: i32) {
    handles()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .swap_remove(&(id as u32));
}
