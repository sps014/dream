//! A minimal DAP client over `dream debug-adapter`'s stdio, shared by the debugger e2e tests.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{mpsc, Mutex};
use std::time::Duration;

pub fn lldb_dap_available() -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        if dir.join("lldb-dap").is_file() || dir.join("lldb-vscode").is_file() {
            return true;
        }
    }
    if cfg!(target_os = "macos")
        && let Ok(out) = Command::new("xcrun").args(["--find", "lldb-dap"]).output()
            && out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                return !s.is_empty() && std::path::Path::new(&s).is_file();
            }
    false
}

pub struct DapClient {
    child: Child,
    stdin: ChildStdin,
    rx: mpsc::Receiver<serde_json::Value>,
    /// Unmatched messages kept so later `wait_for` callers still see events that arrived while
    /// waiting for something else (e.g. `thread` started while waiting for `stopped`).
    pending: Mutex<VecDeque<serde_json::Value>>,
    seq: i64,
}

impl DapClient {
    /// Spawns `dream debug-adapter <args…>`.
    pub fn spawn_with(args: &[&str]) -> DapClient {
        let bin = env!("CARGO_BIN_EXE_dream");
        let mut child = Command::new(bin)
            .arg("debug-adapter")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
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

    pub fn request(&mut self, command: &str, arguments: serde_json::Value) {
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
    pub fn wait_for(&self, pred: impl Fn(&serde_json::Value) -> bool) -> serde_json::Value {
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

    pub fn wait_response(&self, command: &str) -> serde_json::Value {
        self.wait_for(|m| m["type"] == "response" && m["command"] == command)
    }

    pub fn wait_event(&self, event: &str) -> serde_json::Value {
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

pub fn llvm_available() -> bool {
    match dream::execution::llvm::tools::resolve_llvm(&std::sync::Arc::new(
        dream::driver::toolchain::ToolchainConfig::default(),
    )) {
        Ok(_) => true,
        Err(e) => {
            eprintln!("skipping debugger test: {e}");
            false
        }
    }
}
