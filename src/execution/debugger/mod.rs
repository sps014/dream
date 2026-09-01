//! DAP stdio adapter for native C: compiles with DWARF (`#line` → `.dream`) and proxies
//! the session to `lldb-dap`. VS Code keeps `type: dream` and does not switch to a C debugger.

mod protocol;

/// Presentation-only lldb formatters, written next to the compiled artifacts and imported by the
/// proxied session so Dream strings/arrays render as text in the debugger.
const LLDB_FORMATTERS: &str = include_str!("dream_lldb_formatters.py");

/// Extracts debugger-view typedef names from the generated C: lines shaped
/// `typedef struct [...} Name;`. Function-pointer typedefs (containing `(`) are skipped.
fn view_typedef_names(c_src: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in c_src.lines() {
        let line = line.trim();
        // Packed array views carry `__attribute__((packed))`, whose parens are not a function
        // pointer; strip attributes before the fn-ptr exclusion.
        let plain = match line.find("__attribute__") {
            Some(i) => &line[..i],
            None => line,
        };
        if !plain.starts_with("typedef struct") || plain.contains('(') {
            continue;
        }
        if let Some(name) = line
            .rsplit('}')
            .next()
            .and_then(|t| t.trim_end().strip_suffix(';'))
        {
            let name = name.trim();
            if !name.is_empty() {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

fn formatter_init_commands(formatters: &Path) -> Vec<String> {
    vec![format!("command script import {}", formatters.display())]
}

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
    // Presentation-only lldb formatters (strings as quoted text, array lengths), shipped next to
    // the other artifacts and imported at session start via `initCommands`. lldb forbids dots in
    // module names, so the stem joins with underscores rather than `with_extension`.
    let c_path_p = Path::new(c_path);
    let stem = c_path_p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("dream");
    let formatters = c_path_p.with_file_name(format!("{stem}_lldb_dream.py"));
    std::fs::write(&formatters, LLDB_FORMATTERS)?;
    // The import hook reads this generated list to know which typedef names get summaries.
    let names: Vec<String> = std::fs::read_to_string(c_path)
        .map(|src| view_typedef_names(&src))
        .unwrap_or_default();
    let list: Vec<String> = names.iter().map(|n| format!("\"{n}\"")).collect();
    std::fs::write(
        c_path_p.with_file_name(format!("{stem}_lldb_names.py")),
        format!("NAMES = [{}]\n", list.join(", ")),
    )?;

    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    while let Some(mut msg) = read_message(&mut reader)? {
        match msg.get("command").and_then(|c| c.as_str()) {
            Some("launch") => {
                if msg.get("arguments").is_none_or(Value::is_null) {
                    msg["arguments"] = json!({});
                }
                rewrite_launch(&mut msg, &bin_s, &cwd, &env_pairs);
                // Registration must happen as session commands (not from the module's import
                // hook): summaries added during `command script import` do not reliably attach.
                merge_init_commands(&mut msg, &formatter_init_commands(&formatters));
            }
            // DWARF records symlink-resolved source paths (macOS `/tmp` → `/private/tmp`), while
            // clients echo back whatever path they opened; lldb-dap compares literally, so
            // canonicalize before forwarding or breakpoints silently never bind.
            Some("setBreakpoints") | Some("source") => canonicalize_source_paths(&mut msg),
            _ => {}
        }
        write_message(&mut lldb_in, &msg)?;
    }

    drop(lldb_in);
    let _ = child.wait();
    let _ = pump.join();
    Ok(())
}

/// Resolves `path` to its real filesystem spelling; returns the input when resolution fails.
fn canonicalize_path(path: &str) -> String {
    std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string())
}

fn canonicalize_source(msg: &mut Value) {
    let Some(obj) = msg.as_object_mut() else {
        return;
    };
    if let Some(Value::String(path)) = obj.get_mut("path") {
        *path = canonicalize_path(path);
    }
}

/// Rewrites every `source.path` in a request (`setBreakpoints` carries them at the top level and
/// per breakpoint; `source` is one itself).
fn canonicalize_source_paths(msg: &mut Value) {
    // Clients put the file either on the request itself (`source`, `arguments.source`) or
    // per breakpoint (`arguments.breakpoints[].source`) depending on request kind.
    for loc in ["source", "/arguments/source"] {
        if msg.pointer(loc).is_some() {
            canonicalize_source(msg.pointer_mut(loc).expect("checked above"));
        }
    }
    let Some(bps) = msg
        .pointer_mut("/arguments/breakpoints")
        .and_then(|b| b.as_array_mut())
    else {
        return;
    };
    for bp in bps {
        if bp.get("source").is_some() {
            canonicalize_source(&mut bp["source"]);
        }
    }
}

/// Merges extra lldb commands into the launch request's `initCommands`, preserving any the
/// client already supplied.
fn merge_init_commands(msg: &mut Value, extra: &[String]) {
    let args = msg
        .as_object_mut()
        .and_then(|o| o.get_mut("arguments"))
        .and_then(|a| a.as_object_mut());
    let Some(args) = args else {
        return;
    };
    let mut commands: Vec<Value> = args
        .get("initCommands")
        .and_then(|c| c.as_array().cloned())
        .unwrap_or_default();
    commands.extend(extra.iter().map(|c| json!(c)));
    args.insert("initCommands".into(), Value::Array(commands));
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
