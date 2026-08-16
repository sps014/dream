use crate::guest;
use indexmap::IndexMap;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

struct StreamBuf {
    data: Mutex<VecDeque<u8>>,
    eof: AtomicBool,
}

impl StreamBuf {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            data: Mutex::new(VecDeque::new()),
            eof: AtomicBool::new(false),
        })
    }
}

fn spawn_reader<R: Read + Send + 'static>(mut reader: R, buf: Arc<StreamBuf>) {
    thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    buf.data
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .extend(chunk[..n].iter().copied());
                }
                Err(_) => break,
            }
        }
        buf.eof.store(true, Ordering::SeqCst);
    });
}

fn read_stream_bytes(buf: &StreamBuf, max_bytes: usize) -> Vec<u8> {
    loop {
        {
            let mut data = buf.data.lock().unwrap_or_else(|e| e.into_inner());
            if !data.is_empty() {
                let take = max_bytes.min(data.len());
                return data.drain(..take).collect();
            }
            if buf.eof.load(Ordering::SeqCst) {
                return Vec::new();
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
}

fn read_stream_line(buf: &StreamBuf) -> Option<Vec<u8>> {
    loop {
        {
            let mut data = buf.data.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(pos) = data.iter().position(|&b| b == b'\n') {
                let mut line: Vec<u8> = data.drain(..=pos).collect();
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return Some(line);
            }
            if buf.eof.load(Ordering::SeqCst) {
                if data.is_empty() {
                    return None;
                }
                return Some(data.drain(..).collect());
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
}

struct ChildEntry {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: Arc<StreamBuf>,
    stderr: Arc<StreamBuf>,
}

fn children() -> &'static Mutex<IndexMap<u32, ChildEntry>> {
    static H: OnceLock<Mutex<IndexMap<u32, ChildEntry>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(IndexMap::new()))
}

static NEXT: AtomicU32 = AtomicU32::new(1);

fn split_args(joined: &str) -> Vec<String> {
    if joined.is_empty() {
        Vec::new()
    } else {
        joined.split('\n').map(str::to_string).collect()
    }
}

fn configure(cmd: &str, args: &[String], cwd: &str) -> Command {
    let mut c = Command::new(cmd);
    c.args(args);
    if !cwd.is_empty() {
        c.current_dir(cwd);
    }
    c
}

fn wire_header(header: impl std::fmt::Display, tail: &[u8]) -> Vec<u8> {
    let mut out = format!("{header}\n").into_bytes();
    out.extend_from_slice(tail);
    out
}

#[no_mangle]
pub extern "C" fn dream_process_run(cmd: i32, args: i32, cwd: i32) -> i32 {
    let cmd = guest::read_string(cmd);
    let args = split_args(&guest::read_string(args));
    let cwd = guest::read_string(cwd);
    let wire = match configure(&cmd, &args, &cwd).output() {
        Ok(output) => {
            let mut tail = format!("{}\n", output.stdout.len()).into_bytes();
            tail.extend_from_slice(&output.stdout);
            tail.extend_from_slice(&output.stderr);
            wire_header(output.status.code().unwrap_or(-2), &tail)
        }
        Err(e) => wire_header(-1, e.to_string().as_bytes()),
    };
    guest::write_bytes(&wire)
}

#[no_mangle]
pub extern "C" fn dream_process_spawn(cmd: i32, args: i32, cwd: i32) -> i32 {
    let cmd = guest::read_string(cmd);
    let args = split_args(&guest::read_string(args));
    let cwd = guest::read_string(cwd);
    let mut command = configure(&cmd, &args, &cwd);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let wire = match command.spawn() {
        Ok(mut child) => {
            let stdin = child.stdin.take();
            let stdout = StreamBuf::new();
            let stderr = StreamBuf::new();
            if let Some(p) = child.stdout.take() {
                spawn_reader(p, stdout.clone());
            }
            if let Some(p) = child.stderr.take() {
                spawn_reader(p, stderr.clone());
            }
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            children().lock().unwrap_or_else(|e| e.into_inner()).insert(
                id,
                ChildEntry {
                    child,
                    stdin,
                    stdout,
                    stderr,
                },
            );
            wire_header(id, &[])
        }
        Err(e) => wire_header(-1, e.to_string().as_bytes()),
    };
    guest::write_bytes(&wire)
}

#[no_mangle]
pub extern "C" fn dream_process_write_stdin(handle: i32, data: i32) -> i32 {
    let bytes = guest::read_bytes(data);
    let mut table = children().lock().unwrap_or_else(|e| e.into_inner());
    table
        .get_mut(&(handle as u32))
        .and_then(|e| e.stdin.as_mut())
        .map(|s| s.write_all(&bytes).is_ok())
        .unwrap_or(false) as i32
}

#[no_mangle]
pub extern "C" fn dream_process_read_stream(handle: i32, stream: i32, max_bytes: i32) -> i32 {
    let buf = {
        let table = children().lock().unwrap_or_else(|e| e.into_inner());
        table.get(&(handle as u32)).map(|e| {
            if stream == 1 {
                e.stderr.clone()
            } else {
                e.stdout.clone()
            }
        })
    };
    let bytes = match buf {
        Some(b) => read_stream_bytes(&b, max_bytes.max(0) as usize),
        None => Vec::new(),
    };
    guest::write_bytes(&bytes)
}

#[no_mangle]
pub extern "C" fn dream_process_read_stream_line(handle: i32, stream: i32) -> i32 {
    let buf = {
        let table = children().lock().unwrap_or_else(|e| e.into_inner());
        table.get(&(handle as u32)).map(|e| {
            if stream == 1 {
                e.stderr.clone()
            } else {
                e.stdout.clone()
            }
        })
    };
    let wire = match buf.and_then(|b| read_stream_line(&b)) {
        Some(line) => {
            let mut out = vec![b'1'];
            out.extend_from_slice(&line);
            out
        }
        None => vec![b'0'],
    };
    guest::write_bytes(&wire)
}

#[no_mangle]
pub extern "C" fn dream_process_wait(handle: i32) -> i32 {
    let code = {
        let mut table = children().lock().unwrap_or_else(|e| e.into_inner());
        match table.get_mut(&(handle as u32)) {
            Some(e) => match e.child.wait() {
                Ok(s) => s.code().unwrap_or(-2),
                Err(_) => -1,
            },
            None => -1,
        }
    };
    guest::write_bytes(code.to_string().as_bytes())
}

#[no_mangle]
pub extern "C" fn dream_process_kill(handle: i32) -> i32 {
    children()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_mut(&(handle as u32))
        .map(|e| e.child.kill().is_ok())
        .unwrap_or(false) as i32
}
