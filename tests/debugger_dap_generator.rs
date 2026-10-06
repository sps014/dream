//! DAP e2e for `dream debug-adapter --generator`: the program's compile runs the generator's
//! cached debug executable under lldb-dap, so breakpoints in generator source stop.

use crate::dap::{DapClient, lldb_dap_available, llvm_available};
use std::path::Path;

const GENERATOR: &str = r#"module gen;

import system.codegen;

@generator
@syntax_block
public fun quote(ctx: GenContext): void {
    for (let block in ctx.syntax_blocks()) {
        let text = block.body.trim();
        ctx.replace(block, "\"" + text + "\"");
    }
}
"#;

const PROGRAM: &str = r#"import system;
import gen;

fun main(): void {
    System.println(quote { hello });
}
"#;

const JSON_PROGRAM: &str = r#"import system;
import system.json;

@json
class Reply {
    public ok: bool;
    public constructor() { this.ok = false; }
}

fun main(): void {
    System.println(Json.serialize(Reply()));
}
"#;

fn write_files(tag: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("dream_dap_gen_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (name, text) in files {
        std::fs::write(dir.join(name), text).unwrap();
    }
    dir
}

/// Runs a generator session to a breakpoint at `bp_file:line` and returns the names of the
/// innermost frame's locals after asserting it stopped there.
fn locals_at_generator_breakpoint(
    program: &Path,
    generator: &str,
    bp_file: &Path,
    line: u32,
) -> Vec<String> {
    let program = program.to_string_lossy().into_owned();
    let mut client = DapClient::spawn_with(&[&program, "--generator", generator]);
    client.request(
        "initialize",
        serde_json::json!({ "adapterID": "dream", "linesStartAt1": true, "pathFormat": "path" }),
    );
    client.wait_response("initialize");
    client.request("launch", serde_json::json!({ "program": program }));
    client.wait_response("launch");
    client.wait_event("initialized");
    client.request(
        "setBreakpoints",
        serde_json::json!({
            "source": { "path": bp_file.to_string_lossy() },
            "breakpoints": [ { "line": line } ],
        }),
    );
    client.wait_response("setBreakpoints");
    client.request("configurationDone", serde_json::json!({}));
    client.wait_response("configurationDone");

    let stopped = client.wait_event("stopped");
    assert_eq!(stopped["body"]["reason"], "breakpoint");
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
    let top = &frames[0];
    assert_eq!(top["line"], line, "{:?}", frames);
    let shown = top["source"]["path"].as_str().unwrap_or_default();
    let want = bp_file.file_name().unwrap().to_string_lossy();
    assert!(
        shown.ends_with(want.as_ref()),
        "frame source {} is not {}",
        shown,
        want
    );

    client.request("scopes", serde_json::json!({ "frameId": top["id"] }));
    let scopes = client.wait_response("scopes");
    let reference = scopes["body"]["scopes"][0]["variablesReference"].clone();
    client.request(
        "variables",
        serde_json::json!({ "variablesReference": reference }),
    );
    let vars = client.wait_response("variables");
    let names = vars["body"]["variables"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v["name"].as_str().map(String::from))
        .collect();
    client.request("continue", serde_json::json!({ "threadId": thread_id }));
    client.wait_response("continue");
    client.wait_event("terminated");
    names
}

#[test]
#[ignore = "spawns debug-adapter; cargo test --workspace -- --ignored"]
fn dap_breakpoint_in_user_generator() {
    if !llvm_available() || !lldb_dap_available() {
        eprintln!("skipping: needs the pinned LLVM and lldb-dap");
        return;
    }
    let dir = write_files("user", &[("gen.dream", GENERATOR), ("prog.dream", PROGRAM)]);
    // Line 10 is `ctx.replace(...)`, after `text` is bound.
    let names = locals_at_generator_breakpoint(
        &dir.join("prog.dream"),
        "quote",
        &dir.join("gen.dream"),
        10,
    );
    assert!(names.iter().any(|n| n == "text"), "locals: {:?}", names);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "spawns debug-adapter; cargo test --workspace -- --ignored"]
fn dap_breakpoint_in_std_generator_source() {
    if !llvm_available() || !lldb_dap_available() {
        eprintln!("skipping: needs the pinned LLVM and lldb-dap");
        return;
    }
    let config = dream::driver::toolchain::ToolchainConfig::default();
    let std_dir = dream::driver::std_sources::materialize(&config.std_sources_root()).unwrap();
    let derive = std_dir.join("system/json/derive/json_derive.dream");
    let text = std::fs::read_to_string(&derive).unwrap();
    let line = text
        .lines()
        .position(|l| l.contains("types.push(spec);"))
        .expect("json_derive pushes each type spec") as u32
        + 1;
    let dir = write_files("std", &[("prog.dream", JSON_PROGRAM)]);
    let names =
        locals_at_generator_breakpoint(&dir.join("prog.dream"), "json_derive", &derive, line);
    assert!(names.iter().any(|n| n == "spec"), "locals: {:?}", names);
    let _ = std::fs::remove_dir_all(dir);
}
