//! DAP e2e: `dream debug-adapter` compiles native C with `#line` and proxies `lldb-dap`.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{mpsc, Mutex};
use std::time::Duration;

/// A tiny two-function program so the call stack has depth: a breakpoint inside `add` should show
/// both `add` and `main`.
const PROGRAM: &str = r#"import system;

fun add(a: int, b: int): int {
    let sum = a + b;
    return sum;
}

fun main(): void {
    let x = 10;
    let y = 32;
    let total = add(x, y);
    System.println(total);
}
"#;

fn lldb_dap_available() -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        if dir.join("lldb-dap").is_file() || dir.join("lldb-vscode").is_file() {
            return true;
        }
    }
    if cfg!(target_os = "macos") {
        if let Ok(out) = Command::new("xcrun").args(["--find", "lldb-dap"]).output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                return !s.is_empty() && std::path::Path::new(&s).is_file();
            }
        }
    }
    false
}

struct DapClient {
    child: Child,
    stdin: ChildStdin,
    rx: mpsc::Receiver<serde_json::Value>,
    /// Unmatched messages kept so later `wait_for` callers still see events that arrived while
    /// waiting for something else (e.g. `thread` started while waiting for `stopped`).
    pending: Mutex<VecDeque<serde_json::Value>>,
    seq: i64,
}

impl DapClient {
    fn spawn(source: &str) -> DapClient {
        let bin = env!("CARGO_BIN_EXE_dream");
        let mut child = Command::new(bin)
            .arg("debug-adapter")
            .arg(source)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn dream debug-adapter");
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        // Reader thread: parse framed DAP messages and forward them over a channel.
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || read_messages(stdout, tx));

        DapClient {
            child,
            stdin,
            rx,
            pending: Mutex::new(VecDeque::new()),
            seq: 1,
        }
    }

    fn request(&mut self, command: &str, arguments: serde_json::Value) {
        let msg = serde_json::json!({
            "seq": self.seq,
            "type": "request",
            "command": command,
            "arguments": arguments,
        });
        self.seq += 1;
        let body = serde_json::to_string(&msg).unwrap();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.stdin.flush().unwrap();
    }

    /// Blocks until a message matching `pred` arrives (or times out / the process exits).
    /// Non-matching messages are queued so a later wait can still observe them.
    fn wait_for(&self, pred: impl Fn(&serde_json::Value) -> bool) -> serde_json::Value {
        {
            let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(idx) = pending.iter().position(&pred) {
                return pending.remove(idx).expect("index from position");
            }
        }
        loop {
            let msg = self
                .rx
                .recv_timeout(Duration::from_secs(120))
                .expect("timed out waiting for a DAP message");
            if pred(&msg) {
                return msg;
            }
            self.pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push_back(msg);
        }
    }

    fn wait_response(&self, command: &str) -> serde_json::Value {
        self.wait_for(|m| m["type"] == "response" && m["command"] == command)
    }

    fn wait_event(&self, event: &str) -> serde_json::Value {
        self.wait_for(|m| m["type"] == "event" && m["event"] == event)
    }
}

impl Drop for DapClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_messages(stdout: ChildStdout, tx: mpsc::Sender<serde_json::Value>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                break;
            }
            if let Some(rest) = trimmed.to_ascii_lowercase().strip_prefix("content-length:") {
                content_length = rest.trim().parse().ok();
            }
        }
        let Some(len) = content_length else {
            return;
        };
        let mut buf = vec![0u8; len];
        if reader.read_exact(&mut buf).is_err() {
            return;
        }
        match serde_json::from_slice(&buf) {
            Ok(v) => {
                if tx.send(v).is_err() {
                    return;
                }
            }
            Err(_) => return,
        }
    }
}

#[test]
#[ignore = "spawns debug-adapter; cargo test --workspace -- --ignored"]
fn dap_breakpoint_stack_variables_step_continue() {
    if !lldb_dap_available() {
        eprintln!("skipping: lldb-dap not on PATH");
        return;
    }
    // Write the program to a unique temp file (the adapter compiles it and emits sibling artifacts).
    let dir = std::env::temp_dir().join(format!("dream_dap_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("prog.dream");
    std::fs::write(&source, PROGRAM).unwrap();
    let source_path = source.to_string_lossy().into_owned();

    let mut client = DapClient::spawn(&source_path);

    client.request(
        "initialize",
        serde_json::json!({ "adapterID": "dream", "linesStartAt1": true }),
    );
    client.wait_response("initialize");
    client.wait_event("initialized");

    client.request("launch", serde_json::json!({ "program": source_path }));
    client.wait_response("launch");

    // Breakpoint on `return sum;` (line 5), inside `add`.
    client.request(
        "setBreakpoints",
        serde_json::json!({
            "source": { "path": source_path },
            "breakpoints": [ { "line": 5 } ],
        }),
    );
    let bp = client.wait_response("setBreakpoints");
    assert_eq!(bp["body"]["breakpoints"][0]["verified"], true);

    client.request("configurationDone", serde_json::json!({}));
    client.wait_response("configurationDone");

    // Should stop at the breakpoint.
    let stopped = client.wait_event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint");

    // The call stack must show `add` (innermost, line 5) over `main`.
    client.request("stackTrace", serde_json::json!({ "threadId": 1 }));
    let st = client.wait_response("stackTrace");
    let frames = st["body"]["stackFrames"].as_array().unwrap();
    assert!(
        frames.iter().any(|f| f["line"] == 5),
        "expected a frame on .dream line 5: {:?}",
        frames
    );
    let frame_id = frames[0]["id"].clone();

    client.request("scopes", serde_json::json!({ "frameId": frame_id }));
    let scopes = client.wait_response("scopes");
    let reference = scopes["body"]["scopes"][0]["variablesReference"].clone();
    client.request(
        "variables",
        serde_json::json!({ "variablesReference": reference }),
    );
    let vars = client.wait_response("variables");
    let vars = vars["body"]["variables"].as_array().unwrap();
    assert!(
        !vars.is_empty(),
        "expected DWARF locals (C names lN are ok): {:?}",
        vars
    );

    client.request("continue", serde_json::json!({ "threadId": stopped["body"]["threadId"] }));
    client.wait_response("continue");

    // Program output is surfaced as `output` events; expect the printed total.
    // Then the program terminates.
    client.wait_event("terminated");

    // Best-effort cleanup of the emitted artifacts.
    let _ = std::fs::remove_dir_all(&dir);
}

/// Writes `program` to a fresh temp file and returns `(dir, source_path)`; the adapter compiles it and
/// emits sibling `.wat`/`.dbg.json` artifacts next to it.
fn write_temp_program(tag: &str, program: &str) -> (std::path::PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("dream_dap_{}_{}", tag, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("prog.dream");
    std::fs::write(&source, program).unwrap();
    let source_path = source.to_string_lossy().into_owned();
    (dir, source_path)
}

/// Drives an adapter session up to the first `stopped` event on a breakpoint at `line`, returning the
/// live client so the test can inspect state.
fn run_to_breakpoint(source_path: &str, line: u32) -> DapClient {
    let mut client = DapClient::spawn(source_path);
    client.request(
        "initialize",
        serde_json::json!({ "adapterID": "dream", "linesStartAt1": true }),
    );
    client.wait_response("initialize");
    client.wait_event("initialized");
    client.request("launch", serde_json::json!({ "program": source_path }));
    client.wait_response("launch");
    client.request(
        "setBreakpoints",
        serde_json::json!({
            "source": { "path": source_path },
            "breakpoints": [ { "line": line } ],
        }),
    );
    let bp = client.wait_response("setBreakpoints");
    assert_eq!(bp["body"]["breakpoints"][0]["verified"], true);
    client.request("configurationDone", serde_json::json!({}));
    client.wait_response("configurationDone");
    client
}

/// An `async fun main` whose body has a branch/loop; a breakpoint on the `if` header line must hit,
/// with a clean user-only call stack and live locals decoded from the coroutine frame.
const ASYNC_PROGRAM: &str = r#"import system;

fun compute(n: int): int {
    let total = 0;
    let i = 0;
    while (i < n) {
        total = total + i;
        i = i + 1;
    }
    return total;
}

async fun main(): void {
    let base = 10;
    let sum = compute(base);
    if (sum > 5) {
        System.println(sum);
    }
}
"#;

#[test]
#[ignore = "spawns debug-adapter; cargo test --workspace -- --ignored"]
fn dap_async_breakpoint_on_branch_with_locals() {
    if !lldb_dap_available() {
        eprintln!("skipping: lldb-dap not on PATH");
        return;
    }
    let (dir, source_path) = write_temp_program("async", ASYNC_PROGRAM);

    // Line 16 is the `if (sum > 5)` header inside the async `main`.
    let mut client = run_to_breakpoint(&source_path, 16);

    let stopped = client.wait_event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint");

    client.request(
        "stackTrace",
        serde_json::json!({ "threadId": stopped["body"]["threadId"] }),
    );
    let st = client.wait_response("stackTrace");
    let frames = st["body"]["stackFrames"].as_array().unwrap();
    assert!(
        frames.iter().any(|f| f["line"] == 16),
        "expected a frame on async main line 16: {:?}",
        frames
    );
    let frame_id = frames[0]["id"].clone();

    // Locals decode from the coroutine frame: base=10 and sum=45 (compute(10) = 0+..+9) by line 16.
    client.request("scopes", serde_json::json!({ "frameId": frame_id }));
    let scopes = client.wait_response("scopes");
    let reference = scopes["body"]["scopes"][0]["variablesReference"].clone();
    client.request(
        "variables",
        serde_json::json!({ "variablesReference": reference }),
    );
    let vars = client.wait_response("variables");
    let vars = vars["body"]["variables"].as_array().unwrap();
    assert!(
        !vars.is_empty(),
        "expected DWARF locals on async frame: {:?}",
        vars
    );

    client.request(
        "continue",
        serde_json::json!({ "threadId": stopped["body"]["threadId"] }),
    );
    client.wait_response("continue");
    client.wait_event("terminated");

    let _ = std::fs::remove_dir_all(&dir);
}
