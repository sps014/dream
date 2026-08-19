//! DAP stdio adapter for native C: compiles with DWARF (`#line` → `.dream`) and proxies
//! the session to `lldb-dap`. VS Code keeps `type: dream` and does not switch to a C debugger.

mod protocol;

use protocol::{read_message, write_message};
use serde_json::{json, Value};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

/// Speak DAP over stdin/stdout by driving `lldb-dap` on `bin` (guest + runtime built with `-g`).
pub fn run_debug_adapter(bin: &Path, c_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let dap = find_lldb_dap()?;
    let mut child = Command::new(&dap)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| {
            format!(
                "failed to spawn {}: {e}\n{}",
                dap.display(),
                lldb_dap_hint()
            )
        })?;

    let mut lldb_in = child.stdin.take().ok_or("lldb-dap missing stdin")?;
    let mut lldb_out = child.stdout.take().ok_or("lldb-dap missing stdout")?;

    let pump = thread::spawn(move || {
        let mut stdout = io::stdout();
        let mut buf = [0u8; 8192];
        loop {
            match lldb_out.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if stdout.write_all(&buf[..n]).is_err() {
                        break;
                    }
                    let _ = stdout.flush();
                }
                Err(_) => break,
            }
        }
    });

    let bin_s = bin.to_string_lossy().into_owned();
    let env_pairs = crate::execution::native_c::native_run_env_pairs(c_path);
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    while let Some(mut msg) = read_message(&mut reader)? {
        if msg.get("command").and_then(|c| c.as_str()) == Some("launch") {
            if msg.get("arguments").is_none_or(Value::is_null) {
                msg["arguments"] = json!({});
            }
            rewrite_launch(&mut msg, &bin_s, &cwd, &env_pairs);
        }
        write_message(&mut lldb_in, &msg)?;
    }

    drop(lldb_in);
    let _ = child.wait();
    let _ = pump.join();
    Ok(())
}

fn rewrite_launch(msg: &mut Value, bin: &str, cwd: &Path, env_pairs: &[(String, String)]) {
    let args = msg
        .as_object_mut()
        .and_then(|o| o.get_mut("arguments"))
        .and_then(|a| a.as_object_mut());
    let Some(args) = args else {
        return;
    };
    args.insert("program".into(), json!(bin));
    if !args.contains_key("cwd") {
        args.insert("cwd".into(), json!(cwd.to_string_lossy()));
    }
    let mut env_obj = serde_json::Map::new();
    let mut env_arr = Vec::new();
    for (k, v) in env_pairs {
        env_obj.insert(k.clone(), json!(v));
        env_arr.push(format!("{k}={v}"));
    }
    args.insert("env".into(), Value::Object(env_obj));
    args.insert("envArray".into(), json!(env_arr));
}

fn find_lldb_dap() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(p) = which("lldb-dap") {
        return Ok(p);
    }
    if let Some(p) = which("lldb-vscode") {
        return Ok(p);
    }
    if cfg!(target_os = "macos") {
        if let Ok(out) = Command::new("xcrun").args(["--find", "lldb-dap"]).output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() {
                    let p = PathBuf::from(s);
                    if p.is_file() {
                        return Ok(p);
                    }
                }
            }
        }
    }
    Err(lldb_dap_hint().into())
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(name);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

fn lldb_dap_hint() -> String {
    "debug-adapter needs lldb-dap (LLVM/Xcode). Install with `xcode-select --install` \
     (macOS), your distro's `lldb` package, or the CodeLLDB VS Code extension's bundled adapter."
        .to_string()
}
