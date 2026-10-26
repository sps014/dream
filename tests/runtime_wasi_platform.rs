#![cfg(feature = "native")]

use std::fs;
use std::process::Command;

#[test]
fn browser_diagnostics_decode_shared_memory_through_owned_bytes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let script = r#"
const fs = require('node:fs');
const assert = require('node:assert/strict');
const source = fs.readFileSync(process.argv[1], 'utf8').split('\n').slice(1).join('\n');
class BrowserDecoder extends TextDecoder {
  decode(input, options) {
    if (input && input.buffer instanceof SharedArrayBuffer) throw new TypeError('shared input');
    return super.decode(input, options);
  }
}
const factory = new Function('getNodeFs', 'TextDecoder', source + '; return defaultEnv;')(() => null, BrowserDecoder);
const bytes = new Uint8Array(new SharedArrayBuffer(64));
const out = [], err = [];
const env = factory(() => ({bytes}), {stdout: text => out.push(text), stderr: text => err.push(text)});
bytes.set(new TextEncoder().encode('hello'));
env.write_text(1, 0, 5, 0);
bytes.set([0xAC, 0x20]);
env.write_text(2, 0, 1, 1);
bytes.set([0xF0, 0x9F, 0x8E, 0xB5]);
assert.equal(env.console_write_bytes(1, 0, 2), 2);
assert.equal(env.console_write_bytes(1, 2, 2), 2);
env.console_flush_stream(1);
assert.equal(out.join(''), 'hello🎵');
assert.equal(err.join(''), '€');
"#;
    let output = Command::new("node")
        .arg("-e")
        .arg(script)
        .arg(root.join("runtime/src/hosts/env.js"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn release_guest_resolves_synthesized_libc_calls_internally() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("weak.dream");
    let output = temporary.path().join("weak.wat");
    fs::copy(
        crate::common::compiler_fixture("weak_field_runtime"),
        &source,
    )
    .unwrap();
    dream::driver::compiler::Compiler::new(dream_mir::backend::Target::wasm32())
        .with_release(true)
        .compile(
            &source.to_str().unwrap().to_string(),
            output.to_str().unwrap(),
        )
        .unwrap();
    let bytes = fs::read(output.with_extension("wasm")).unwrap();
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::ImportSection(imports) = payload.unwrap() {
            for import in imports.into_imports() {
                let import = import.unwrap();
                assert!(
                    ![
                        "malloc", "calloc", "realloc", "free", "memcpy", "memmove", "memset",
                        "memcmp", "strlen"
                    ]
                    .contains(&import.name),
                    "guest imports libc `{}` from `{}`",
                    import.name,
                    import.module
                );
            }
        }
    }
}

#[test]
fn wasi_page_exhaustion_reports_through_the_platform_without_allocating() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("empty.dream");
    let output = temporary.path().join("empty.wat");
    fs::write(&source, "import system;\n@export fun live_count(): long { return Debug.live_objects; }\n@export fun total_count(): long { return Debug.total_allocations; }\nfun main(): void {}\n").unwrap();
    dream::driver::compiler::Compiler::new(dream_mir::backend::Target::wasm32())
        .compile(
            &source.to_str().unwrap().to_string(),
            output.to_str().unwrap(),
        )
        .unwrap();
    let wasm = output.with_extension("wasm");
    let bytes = fs::read(&wasm).unwrap();
    let mut minimum = None;
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::ImportSection(imports) = payload.unwrap() {
            for import in imports.into_imports() {
                if let wasmparser::TypeRef::Memory(memory) = import.unwrap().ty {
                    minimum = Some(memory.initial);
                }
            }
        }
    }
    let minimum = minimum.expect("guest imports linear memory");
    let runner = temporary.path().join("exhaustion.mjs");
    let wasm_path = serde_json::to_string(&wasm.to_str().unwrap()).unwrap();
    fs::write(
        &runner,
        format!(
            r#"import {{readFileSync}} from 'node:fs';
import assert from 'node:assert/strict';
const module = await WebAssembly.compile(readFileSync({wasm_path}));
const memory = new WebAssembly.Memory({{initial: {minimum}, maximum: {minimum} + 64}});
let stderr = '';
const imports = {{env: {{memory}}}};
for (const entry of WebAssembly.Module.imports(module)) {{
    if (entry.kind === 'function') {{
        (imports[entry.module] ??= {{}})[entry.name] = () => 0;
    }}
}}
imports.env.write_text = (stream, ptr, length, encoding) => {{
    assert.equal(stream, 2);
    const bytes = new Uint8Array(memory.buffer, ptr, length * (encoding === 1 ? 2 : 1));
    stderr += new TextDecoder(encoding === 1 ? 'utf-16le' : 'utf-8').decode(bytes);
}};
const instance = await WebAssembly.instantiate(module, imports);
instance.exports.__runtime_init();
const live = instance.exports.live_count();
for (const size of [1, 17, 33, 65, 129, 511, 1025]) {{
    const allocations = instance.exports.total_count();
    const raw = instance.exports.dream_wasm_raw_malloc(size);
    assert.equal(instance.exports.live_count(), live);
    assert.equal(instance.exports.total_count(), allocations);
    const pointer = instance.exports.malloc(size, 0);
    assert.equal(instance.exports.live_count(), live + 1n);
    instance.exports.dream_wasm_raw_free(raw);
    assert.equal(instance.exports.live_count(), live + 1n);
    assert.equal(instance.exports.total_count(), allocations + 1n);
    instance.exports.free(pointer);
    assert.equal(instance.exports.live_count(), live);
    for (let attempt = 0; attempt < 32; attempt++) {{
        const reused = instance.exports.malloc(size, 0);
        assert.equal(reused, pointer, `small allocation class was not reused: ${{size}}`);
        instance.exports.free(reused);
    }}
}}
const before = memory.buffer.byteLength;
assert.throws(() => instance.exports.malloc(({minimum} + 64) * 65536, 0), WebAssembly.RuntimeError);
assert.equal(memory.buffer.byteLength, before);
assert.equal(stderr, 'panic: out of memory growing the WASI heap\n');
console.log('WASI platform exhaustion passed');
"#
        ),
    )
    .unwrap();
    let result = Command::new("node").arg(runner).output().unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
