use dream_host_abi::operations::Failure;

use crate::service::error;

pub(crate) fn list_processes() -> Result<Vec<u8>, Failure> {
    encode_rows(read_rows()?)
}

fn encode_rows(mut rows: Vec<(i32, i32, String)>) -> Result<Vec<u8>, Failure> {
    rows.sort_by_key(|row| row.0);
    let mut out = format!("{}\n", rows.len());
    for (pid, parent, name) in rows {
        let name = name.replace(['\n', '\r'], " ");
        out.push_str(&format!("{pid}\n{parent}\n{name}\n"));
    }
    Ok(out.into_bytes())
}

#[cfg(target_os = "macos")]
fn c_chars_to_string(bytes: &[libc::c_char]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let owned: Vec<u8> = bytes[..end].iter().map(|byte| *byte as u8).collect();
    String::from_utf8_lossy(&owned).into_owned()
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn read_rows() -> Result<Vec<(i32, i32, String)>, Failure> {
    let mut rows = Vec::new();
    for entry in std::fs::read_dir("/proc").map_err(|err| error("EIO", err))? {
        let entry = entry.map_err(|err| error("EIO", err))?;
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<i32>() else {
            continue;
        };
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        let parent = parent_pid(&stat);
        let name = std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .unwrap_or_default()
            .trim()
            .to_string();
        rows.push((pid, parent, name));
    }
    Ok(rows)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn parent_pid(stat: &str) -> i32 {
    let Some(end) = stat.rfind(')') else {
        return 0;
    };
    stat[end + 1..]
        .split_whitespace()
        .nth(1)
        .and_then(|text| text.parse().ok())
        .unwrap_or(0)
}

#[cfg(target_os = "macos")]
fn read_rows() -> Result<Vec<(i32, i32, String)>, Failure> {
    // libproc fills caller-owned buffers. A short read means that pid disappeared.
    unsafe {
        let guessed = libc::proc_listallpids(std::ptr::null_mut(), 0);
        if guessed < 0 {
            return Err(error("EIO", "cannot list processes"));
        }
        let slots = (guessed as usize / std::mem::size_of::<libc::pid_t>()).saturating_add(8);
        let mut pids = vec![0 as libc::pid_t; slots];
        let wrote = libc::proc_listallpids(
            pids.as_mut_ptr().cast(),
            (pids.len() * std::mem::size_of::<libc::pid_t>()) as i32,
        );
        if wrote < 0 {
            return Err(error("EIO", "cannot list processes"));
        }
        let count = wrote as usize / std::mem::size_of::<libc::pid_t>();
        let mut rows = Vec::with_capacity(count);
        for &pid in pids.iter().take(count) {
            if pid <= 0 {
                continue;
            }
            let mut info = std::mem::zeroed::<libc::proc_bsdinfo>();
            let size = libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut info as *mut libc::proc_bsdinfo).cast(),
                std::mem::size_of::<libc::proc_bsdinfo>() as i32,
            );
            let (parent, name) = if size as usize >= std::mem::size_of::<libc::proc_bsdinfo>() {
                (info.pbi_ppid as i32, c_chars_to_string(&info.pbi_name))
            } else {
                (0, String::new())
            };
            rows.push((pid, parent, name));
        }
        Ok(rows)
    }
}

#[cfg(windows)]
fn read_rows() -> Result<Vec<(i32, i32, String)>, Failure> {
    use std::os::windows::ffi::OsStringExt;
    const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    #[repr(C)]
    struct ProcessEntry {
        size: u32,
        usage: u32,
        pid: u32,
        heap: usize,
        module: u32,
        threads: u32,
        parent: u32,
        priority: i32,
        flags: u32,
        exe: [u16; 260],
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> *mut std::ffi::c_void;
        fn Process32FirstW(snapshot: *mut std::ffi::c_void, entry: *mut ProcessEntry) -> i32;
        fn Process32NextW(snapshot: *mut std::ffi::c_void, entry: *mut ProcessEntry) -> i32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    }
    // The snapshot handle is closed before this function returns.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() || snapshot == (-1isize as *mut std::ffi::c_void) {
            return Err(error("EIO", "cannot list processes"));
        }
        let mut entry = std::mem::zeroed::<ProcessEntry>();
        entry.size = std::mem::size_of::<ProcessEntry>() as u32;
        let mut rows = Vec::new();
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let end = entry
                    .exe
                    .iter()
                    .position(|unit| *unit == 0)
                    .unwrap_or(entry.exe.len());
                let name = std::ffi::OsString::from_wide(&entry.exe[..end])
                    .to_string_lossy()
                    .into_owned();
                rows.push((entry.pid as i32, entry.parent as i32, name));
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
        Ok(rows)
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    windows
)))]
fn read_rows() -> Result<Vec<(i32, i32, String)>, Failure> {
    Err(error(
        "EUNSUPPORTED",
        "process listing is unavailable on this host",
    ))
}
