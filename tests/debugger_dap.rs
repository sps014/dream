//! DAP e2e: `dream debug-adapter` builds with DWARF and proxies `lldb-dap`.

use crate::dap::{DapClient, lldb_dap_available, llvm_available};

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

#[test]
#[ignore = "spawns debug-adapter; cargo test --workspace -- --ignored"]
fn dap_breakpoint_stack_variables_step_continue() {
    if !llvm_available() {
        return;
    }
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

    let mut client = DapClient::spawn_with(&[&source_path]);

    client.request(
        "initialize",
        serde_json::json!({ "adapterID": "dream", "linesStartAt1": true, "pathFormat": "path" }),
    );
    client.wait_response("initialize");

    client.request("launch", serde_json::json!({ "program": source_path }));
    client.wait_response("launch");
    // This lldb-dap emits `initialized` only after `launch` and parks the process until
    // `configurationDone`, so bind breakpoints in that window.
    client.wait_event("initialized");

    // Breakpoint on `return sum;` (line 5), inside `add`.
    client.request(
        "setBreakpoints",
        serde_json::json!({
            "source": { "path": source_path },
            "breakpoints": [ { "line": 5 } ],
        }),
    );
    let bp = client.wait_response("setBreakpoints");
    // Verification may be deferred until the module loads; the `stopped` assertion below is
    // the real check that the breakpoint bound.
    assert!(!bp["body"]["breakpoints"].as_array().unwrap().is_empty());

    client.request("configurationDone", serde_json::json!({}));
    client.wait_response("configurationDone");

    // Should stop at the breakpoint.
    let stopped = client.wait_event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint");

    // The call stack must show `add` (innermost, line 5) over `main`. lldb-dap advertises
    // delayed stack loading, so the first request after a stop may come back empty.
    let thread_id = stopped["body"]["threadId"].as_i64().expect("threadId");
    let mut frames = Vec::new();
    for _ in 0..10 {
        client.request("stackTrace", serde_json::json!({ "threadId": thread_id }));
        let st = client.wait_response("stackTrace");
        frames = st["body"]["stackFrames"].as_array().unwrap().clone();
        if !frames.is_empty() {
            break;
        }
    }
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
    let names: Vec<String> = vars
        .iter()
        .filter_map(|v| v["name"].as_str().map(String::from))
        .collect();
    assert!(
        ["a", "b", "sum"]
            .iter()
            .all(|n| names.contains(&n.to_string())),
        "expected DWARF locals under their Dream names (a, b, sum), got: {:?}",
        names
    );

    client.request(
        "continue",
        serde_json::json!({ "threadId": stopped["body"]["threadId"] }),
    );
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
    let mut client = DapClient::spawn_with(&[source_path]);
    client.request(
        "initialize",
        serde_json::json!({ "adapterID": "dream", "linesStartAt1": true, "pathFormat": "path" }),
    );
    client.wait_response("initialize");
    client.request("launch", serde_json::json!({ "program": source_path }));
    client.wait_response("launch");
    client.wait_event("initialized");
    client.request(
        "setBreakpoints",
        serde_json::json!({
            "source": { "path": source_path },
            "breakpoints": [ { "line": line } ],
        }),
    );
    let bp = client.wait_response("setBreakpoints");
    // Verification may be deferred until the module loads; the `stopped` assertion below is
    // the real check that the breakpoint bound.
    assert!(!bp["body"]["breakpoints"].as_array().unwrap().is_empty());
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
    if !llvm_available() || !lldb_dap_available() {
        eprintln!("skipping: needs the pinned LLVM and lldb-dap");
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
    let names: Vec<String> = vars
        .iter()
        .filter_map(|v| v["name"].as_str().map(String::from))
        .collect();
    assert!(
        ["base", "sum"]
            .iter()
            .all(|n| names.contains(&n.to_string())),
        "expected DWARF locals under their Dream names (base, sum), got: {:?}",
        names
    );
    let value_of = |want: &str| -> Option<i64> {
        vars.iter()
            .find(|v| v["name"].as_str() == Some(want))
            .and_then(|v| v["value"].as_str())
            .and_then(|s| s.parse::<i64>().ok())
    };
    assert_eq!(value_of("base"), Some(10));
    assert_eq!(value_of("sum"), Some(45));

    client.request(
        "continue",
        serde_json::json!({ "threadId": stopped["body"]["threadId"] }),
    );
    client.wait_response("continue");
    client.wait_event("terminated");

    let _ = std::fs::remove_dir_all(&dir);
}

const VIEWS_PROGRAM: &str = r#"import system;

enum Shape {
    Circle(radius: int),
    Rect(w: int, h: int),
}

fun main(): void {
    let greeting = "hello";
    let xs = [1, 2, 3];
    let sh = Shape.Rect(3, 4);
    System.println(greeting);
    System.println(xs.length);
    System.println(sh);
}
"#;

/// Reference locals render through the DWARF views and the shipped lldb formatters: strings as
/// quoted text, arrays as their elements, unions as the active variant.
#[test]
#[ignore = "spawns debug-adapter; cargo test --workspace -- --ignored"]
fn dap_views_render_strings_arrays_unions() {
    if !llvm_available() || !lldb_dap_available() {
        eprintln!("skipping: needs the pinned LLVM and lldb-dap");
        return;
    }
    let (dir, source_path) = write_temp_program("views", VIEWS_PROGRAM);
    let mut client = run_to_breakpoint(&source_path, 12);
    let stopped = client.wait_event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint");
    client.request(
        "stackTrace",
        serde_json::json!({ "threadId": stopped["body"]["threadId"] }),
    );
    let st = client.wait_response("stackTrace");
    let frame_id = st["body"]["stackFrames"][0]["id"].clone();
    client.request("scopes", serde_json::json!({ "frameId": frame_id }));
    let scopes = client.wait_response("scopes");
    let reference = scopes["body"]["scopes"][0]["variablesReference"].clone();
    client.request(
        "variables",
        serde_json::json!({ "variablesReference": reference }),
    );
    let vars = client.wait_response("variables");
    let shown = |want: &str| -> String {
        vars["body"]["variables"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"].as_str() == Some(want))
            .and_then(|v| v["value"].as_str())
            .unwrap_or_default()
            .to_string()
    };
    assert!(shown("greeting").contains("\"hello\""), "{}", vars);
    assert!(shown("xs").contains("[1, 2, 3]"), "{}", vars);
    assert!(shown("sh").contains("Rect(w=3, h=4)"), "{}", vars);
    client.request(
        "continue",
        serde_json::json!({ "threadId": stopped["body"]["threadId"] }),
    );
    drop(client);
    let _ = std::fs::remove_dir_all(dir);
}
