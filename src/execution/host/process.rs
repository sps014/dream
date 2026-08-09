//! Process / platform host functions (`Dream` module behind `System.platform` / `env` / `args` /
//! `cwd`, and `system.process`'s `Process.run`/`Process.spawn`). Browser/Node hosts implement the
//! same names in `runtime/dream.js`.

use std::collections::VecDeque;
use std::env;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use indexmap::IndexMap;
use wasmtime::*;

use super::memory::{
    read_arg_bytes, read_arg_string, resolve_host_future_bytes, write_string_to_memory,
};

/// One direction of a spawned child's piped stdout/stderr: a background thread continuously
/// drains the OS pipe into `data` so the child can never block on a full pipe buffer while the
/// Dream program is busy doing something else (e.g. reading the other stream, or not reading at
/// all). Readers pop from the front of `data`; `eof` is set once the underlying `read` returns 0.
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

/// Spawns a background thread that copies `reader` into `buf` until EOF or an I/O error.
fn spawn_stream_reader<R: Read + Send + 'static>(mut reader: R, buf: Arc<StreamBuf>) {
    thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    let mut data = buf.data.lock().unwrap_or_else(|e| e.into_inner());
                    data.extend(chunk[..n].iter().copied());
                }
                Err(_) => break,
            }
        }
        buf.eof.store(true, Ordering::SeqCst);
    });
}

/// Poll interval while [`read_stream_bytes`]/[`read_stream_line`] wait for more data. Host calls
/// already block the calling task synchronously (mirroring the HTTP host's blocking `reqwest`
/// client), so a short sleep is an acceptable trade-off for not needing a condition variable.
const POLL_INTERVAL: Duration = Duration::from_millis(2);

/// Blocks until at least one byte is available in `buf` or it has reached EOF, then drains up to
/// `max_bytes`. Returns an empty vector at EOF once every buffered byte has been consumed.
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
        thread::sleep(POLL_INTERVAL);
    }
}

/// Blocks until a full `\n`-terminated line (with any trailing `\r` trimmed) is available, or the
/// stream reaches EOF. Returns `None` only when EOF is reached with no more buffered data at all;
/// a trailing partial line at EOF is still returned once.
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
        thread::sleep(POLL_INTERVAL);
    }
}

struct ChildEntry {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: Arc<StreamBuf>,
    stderr: Arc<StreamBuf>,
}

fn child_handles() -> &'static Mutex<IndexMap<u32, ChildEntry>> {
    static HANDLES: OnceLock<Mutex<IndexMap<u32, ChildEntry>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(IndexMap::new()))
}

static NEXT_CHILD_HANDLE: AtomicU32 = AtomicU32::new(1);

/// Splits `joined` (arguments separated by `\n`, the same wire `Process.join_args` writes) into
/// owned argument strings; an empty string means no arguments.
fn split_args(joined: &str) -> Vec<String> {
    if joined.is_empty() {
        Vec::new()
    } else {
        joined.split('\n').map(str::to_string).collect()
    }
}

fn configure_command(cmd: &str, args: &[String], cwd: &str) -> Command {
    let mut command = Command::new(cmd);
    command.args(args);
    if !cwd.is_empty() {
        command.current_dir(cwd);
    }
    command
}

/// Wire encoding shared by `Process.run`/`Process.spawn`'s `ProcessWireReader`: a decimal header
/// line (`\n`-terminated), then raw bytes for the remainder. Kept host-side so every call site
/// (success and failure) goes through one place.
fn wire_header(header: impl std::fmt::Display, tail: &[u8]) -> Vec<u8> {
    let mut out = format!("{header}\n").into_bytes();
    out.extend_from_slice(tail);
    out
}

/// Registers process/platform host functions on `linker`.
pub fn link_process_functions(linker: &mut Linker<()>) -> Result<()> {
    // 0 = Native, 1 = Node, 2 = Browser, 3 = Unknown
    linker.func_wrap("Dream", "processPlatform", || -> i32 { 0 })?;

    // 0 = Unix, 1 = Windows, 2 = Unknown
    linker.func_wrap("Dream", "processOsFamily", || -> i32 {
        if cfg!(windows) {
            1
        } else {
            0
        }
    })?;

    linker.func_wrap(
        "Dream",
        "processArgs",
        |mut caller: Caller<'_, ()>| -> Result<i32> {
            // Skip argv[0] (exe); join remaining args with '\n' (same wire as dirList).
            let joined = env::args().skip(1).collect::<Vec<_>>().join("\n");
            write_string_to_memory(&mut caller, &joined)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "processExePath",
        |mut caller: Caller<'_, ()>| -> Result<i32> {
            let path = env::current_exe()
                .ok()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            write_string_to_memory(&mut caller, &path)
        },
    )?;

    // Tagged: "1" + value when set; "" when unset.
    linker.func_wrap(
        "Dream",
        "processEnvGet",
        |mut caller: Caller<'_, ()>, name_ptr: i32| -> Result<i32> {
            let name = read_arg_string(&mut caller, name_ptr)?;
            let encoded = match env::var(&name) {
                Ok(v) => format!("1{v}"),
                Err(_) => String::new(),
            };
            write_string_to_memory(&mut caller, &encoded)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "processEnvSet",
        |mut caller: Caller<'_, ()>, name_ptr: i32, value_ptr: i32| -> Result<()> {
            let name = read_arg_string(&mut caller, name_ptr)?;
            let value = read_arg_string(&mut caller, value_ptr)?;
            env::set_var(name, value);
            Ok(())
        },
    )?;

    linker.func_wrap(
        "Dream",
        "processCwd",
        |mut caller: Caller<'_, ()>| -> Result<i32> {
            let cwd = env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .display()
                .to_string();
            write_string_to_memory(&mut caller, &cwd)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "processSetCwd",
        |mut caller: Caller<'_, ()>, path_ptr: i32| -> Result<i32> {
            let path = read_arg_string(&mut caller, path_ptr)?;
            Ok(env::set_current_dir(path).is_ok() as i32)
        },
    )?;

    // Wire (see `ProcessWireReader`): "<exit_code>\n<stdout_len>\n" + stdout bytes + stderr bytes.
    // `exit_code == -1` means the process could not be spawned at all; the tail is then just the
    // OS error message. `exit_code == -2` means it exited via a signal with no exit code (Unix).
    linker.func_wrap(
        "Dream",
        "processRun",
        |mut caller: Caller<'_, ()>, cmd_ptr: i32, args_ptr: i32, cwd_ptr: i32| -> Result<i32> {
            let cmd = read_arg_string(&mut caller, cmd_ptr)?;
            let args = split_args(&read_arg_string(&mut caller, args_ptr)?);
            let cwd = read_arg_string(&mut caller, cwd_ptr)?;
            let wire = match configure_command(&cmd, &args, &cwd).output() {
                Ok(output) => {
                    let mut tail = format!("{}\n", output.stdout.len()).into_bytes();
                    tail.extend_from_slice(&output.stdout);
                    tail.extend_from_slice(&output.stderr);
                    wire_header(output.status.code().unwrap_or(-2), &tail)
                }
                Err(e) => wire_header(-1, e.to_string().as_bytes()),
            };
            resolve_host_future_bytes(&mut caller, &wire)
        },
    )?;

    // Wire: "<handle>\n" on success (an empty tail), or "-1\n<message>" when spawning failed.
    linker.func_wrap(
        "Dream",
        "processSpawn",
        |mut caller: Caller<'_, ()>, cmd_ptr: i32, args_ptr: i32, cwd_ptr: i32| -> Result<i32> {
            let cmd = read_arg_string(&mut caller, cmd_ptr)?;
            let args = split_args(&read_arg_string(&mut caller, args_ptr)?);
            let cwd = read_arg_string(&mut caller, cwd_ptr)?;
            let mut command = configure_command(&cmd, &args, &cwd);
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let wire = match command.spawn() {
                Ok(mut child) => {
                    let stdin = child.stdin.take();
                    let stdout_pipe = child.stdout.take();
                    let stderr_pipe = child.stderr.take();
                    let stdout = StreamBuf::new();
                    let stderr = StreamBuf::new();
                    if let Some(pipe) = stdout_pipe {
                        spawn_stream_reader(pipe, stdout.clone());
                    }
                    if let Some(pipe) = stderr_pipe {
                        spawn_stream_reader(pipe, stderr.clone());
                    }
                    let id = NEXT_CHILD_HANDLE.fetch_add(1, Ordering::Relaxed);
                    let mut table = child_handles().lock().unwrap_or_else(|e| e.into_inner());
                    table.insert(
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
            resolve_host_future_bytes(&mut caller, &wire)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "processWriteStdin",
        |mut caller: Caller<'_, ()>, handle: i32, data_ptr: i32| -> Result<i32> {
            let bytes = read_arg_bytes(&mut caller, data_ptr)?;
            let mut table = child_handles().lock().unwrap_or_else(|e| e.into_inner());
            let ok = table
                .get_mut(&(handle as u32))
                .and_then(|entry| entry.stdin.as_mut())
                .map(|stdin| stdin.write_all(&bytes).is_ok())
                .unwrap_or(false);
            Ok(ok as i32)
        },
    )?;

    // `stream`: 0 = stdout, 1 = stderr. Blocks until data or EOF (see `read_stream_bytes`).
    linker.func_wrap(
        "Dream",
        "processReadStream",
        |mut caller: Caller<'_, ()>, handle: i32, stream: i32, max_bytes: i32| -> Result<i32> {
            let buf = {
                let table = child_handles().lock().unwrap_or_else(|e| e.into_inner());
                table.get(&(handle as u32)).map(|entry| {
                    if stream == 1 {
                        entry.stderr.clone()
                    } else {
                        entry.stdout.clone()
                    }
                })
            };
            let bytes = match buf {
                Some(buf) => read_stream_bytes(&buf, max_bytes.max(0) as usize),
                None => Vec::new(),
            };
            resolve_host_future_bytes(&mut caller, &bytes)
        },
    )?;

    // Wire: "1" + line bytes when a line was read, or "0" alone at end-of-file.
    linker.func_wrap(
        "Dream",
        "processReadStreamLine",
        |mut caller: Caller<'_, ()>, handle: i32, stream: i32| -> Result<i32> {
            let buf = {
                let table = child_handles().lock().unwrap_or_else(|e| e.into_inner());
                table.get(&(handle as u32)).map(|entry| {
                    if stream == 1 {
                        entry.stderr.clone()
                    } else {
                        entry.stdout.clone()
                    }
                })
            };
            let wire = match buf.and_then(|buf| read_stream_line(&buf)) {
                Some(line) => {
                    let mut out = vec![b'1'];
                    out.extend_from_slice(&line);
                    out
                }
                None => vec![b'0'],
            };
            resolve_host_future_bytes(&mut caller, &wire)
        },
    )?;

    // Wire: decimal exit code, or a negative sentinel (`-1` = could not be waited on, `-2` =
    // exited via signal with no exit code).
    linker.func_wrap(
        "Dream",
        "processWait",
        |mut caller: Caller<'_, ()>, handle: i32| -> Result<i32> {
            let exit_code = {
                let mut table = child_handles().lock().unwrap_or_else(|e| e.into_inner());
                match table.get_mut(&(handle as u32)) {
                    Some(entry) => match entry.child.wait() {
                        Ok(status) => status.code().unwrap_or(-2),
                        Err(_) => -1,
                    },
                    None => -1,
                }
            };
            resolve_host_future_bytes(&mut caller, exit_code.to_string().as_bytes())
        },
    )?;

    linker.func_wrap(
        "Dream",
        "processKill",
        |_: Caller<'_, ()>, handle: i32| -> Result<i32> {
            let mut table = child_handles().lock().unwrap_or_else(|e| e.into_inner());
            let ok = table
                .get_mut(&(handle as u32))
                .map(|entry| entry.child.kill().is_ok())
                .unwrap_or(false);
            Ok(ok as i32)
        },
    )?;

    Ok(())
}
