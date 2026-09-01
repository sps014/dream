//! Cross-platform `system.process` hosts (`std::process`), matching the JS/C wire formats.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use indexmap::IndexMap;

struct ChildIo {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: Option<BufReader<ChildStdout>>,
    stderr: Option<BufReader<ChildStderr>>,
}

fn children() -> &'static Mutex<IndexMap<u32, ChildIo>> {
    static T: OnceLock<Mutex<IndexMap<u32, ChildIo>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(IndexMap::new()))
}

static NEXT: AtomicU32 = AtomicU32::new(1);

fn lock_map() -> std::sync::MutexGuard<'static, IndexMap<u32, ChildIo>> {
    children().lock().unwrap_or_else(|e| e.into_inner())
}

fn split_args(joined: &str) -> Vec<&str> {
    if joined.is_empty() {
        Vec::new()
    } else {
        joined.split('\n').collect()
    }
}

fn command(cmd: &str, joined_args: &str, cwd: &str) -> Command {
    let mut c = Command::new(cmd);
    c.args(split_args(joined_args));
    if !cwd.is_empty() {
        c.current_dir(cwd);
    }
    c
}

pub(crate) fn process_run(cmd: &str, joined_args: &str, cwd: &str) -> Vec<u8> {
    let mut c = command(cmd, joined_args, cwd);
    c.stdout(Stdio::piped()).stderr(Stdio::piped());
    match c.output() {
        Err(e) => {
            let mut out = b"-1\n".to_vec();
            out.extend_from_slice(e.to_string().as_bytes());
            out
        }
        Ok(output) => {
            let code = output.status.code().unwrap_or(-2);
            let mut wire = format!("{}\n{}\n", code, output.stdout.len()).into_bytes();
            wire.extend_from_slice(&output.stdout);
            wire.extend_from_slice(&output.stderr);
            wire
        }
    }
}

pub(crate) fn process_spawn(cmd: &str, joined_args: &str, cwd: &str) -> Vec<u8> {
    let mut c = command(cmd, joined_args, cwd);
    c.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match c.spawn() {
        Err(e) => {
            let mut out = b"-1\n".to_vec();
            out.extend_from_slice(e.to_string().as_bytes());
            out
        }
        Ok(mut child) => {
            let stdin = child.stdin.take();
            let stdout = child.stdout.take().map(BufReader::new);
            let stderr = child.stderr.take().map(BufReader::new);
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            lock_map().insert(
                id,
                ChildIo {
                    child,
                    stdin,
                    stdout,
                    stderr,
                },
            );
            format!("{id}\n").into_bytes()
        }
    }
}

pub(crate) fn process_write_stdin(handle: i32, data: &[u8]) -> i32 {
    let mut table = lock_map();
    let Some(child) = table.get_mut(&(handle as u32)) else {
        return 0;
    };
    let Some(stdin) = child.stdin.as_mut() else {
        return 0;
    };
    i32::from(stdin.write_all(data).is_ok() && stdin.flush().is_ok())
}

pub(crate) fn process_read_stream(handle: i32, stream: i32, max_bytes: i32) -> Vec<u8> {
    let cap = (max_bytes.max(0) as usize).min(4096);
    let mut table = lock_map();
    let Some(child) = table.get_mut(&(handle as u32)) else {
        return Vec::new();
    };
    let reader: &mut dyn Read = if stream == 0 {
        match child.stdout.as_mut() {
            Some(r) => r,
            None => return Vec::new(),
        }
    } else {
        match child.stderr.as_mut() {
            Some(r) => r,
            None => return Vec::new(),
        }
    };
    if cap == 0 {
        return Vec::new();
    }
    let mut buf = vec![0u8; cap];
    match reader.read(&mut buf) {
        Ok(n) if n > 0 => buf[..n].to_vec(),
        _ => Vec::new(),
    }
}

pub(crate) fn process_read_stream_line(handle: i32, stream: i32) -> Vec<u8> {
    let mut table = lock_map();
    let Some(child) = table.get_mut(&(handle as u32)) else {
        return b"0".to_vec();
    };
    let mut line = String::new();
    let n = if stream == 0 {
        match child.stdout.as_mut() {
            Some(r) => r.read_line(&mut line),
            None => return b"0".to_vec(),
        }
    } else {
        match child.stderr.as_mut() {
            Some(r) => r.read_line(&mut line),
            None => return b"0".to_vec(),
        }
    };
    match n {
        Ok(0) => b"0".to_vec(),
        Ok(_) => {
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            let mut out = Vec::with_capacity(1 + line.len());
            out.push(b'1');
            out.extend_from_slice(line.as_bytes());
            out
        }
        Err(_) => b"0".to_vec(),
    }
}

pub(crate) fn process_wait(handle: i32) -> Vec<u8> {
    let mut table = lock_map();
    let Some(mut child) = table.shift_remove(&(handle as u32)) else {
        return b"-1".to_vec();
    };
    drop(table);
    match child.child.wait() {
        Ok(status) => status.code().unwrap_or(-2).to_string().into_bytes(),
        Err(_) => b"-1".to_vec(),
    }
}

pub(crate) fn process_kill(handle: i32) -> i32 {
    let mut table = lock_map();
    let Some(child) = table.get_mut(&(handle as u32)) else {
        return 0;
    };
    i32::from(child.child.kill().is_ok())
}
