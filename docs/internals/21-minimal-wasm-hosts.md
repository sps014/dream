# Minimal WASM hosts — implementation tracker

Track the complete download: main WASM, generated JavaScript, required codec modules,
and shader assets. Measurements include raw bytes, gzip level 9, and Brotli quality 11.

## Phase 1 — common ABI and native-only CLI

- [x] Remove the JS CLI host, parser module, WASM builder, and parser CI jobs.
- [x] Keep CLI parsing native-only and migrate its assertions to the native corpus.
- [x] Generate versioned C, Rust, and JS guest contracts from `dream-abi`.
- [x] Bind native capabilities through ABI 3 and opaque guest references.
- [x] Centralize string/array access, allocation, truncation, retain/release, future status, and completion in C callbacks.
- [x] Migrate native window, GPU, filesystem, networking, codecs, crypto, process, Unicode, and timezone adapters to the shared access helpers.
- [x] Check versions before guest allocation and test UTF-16, spans, integer widths, and memory growth.

The guest contract is version 1; native capability binding is version 3. Native callbacks
use native pointers. Browser exports expose the same operations with wasm32 offsets.
Strings contain UTF-16 code units, including embedded zeroes; arrays carry element counts.
Synchronous calls borrow input. Deferred operations own snapshots. Asynchronous adapters
pin their future until completion or cancellation makes it safe to release.

## Phase 2 — minimal browser and Node hosts

- [x] Select host chunks and instance helpers from final live imports.
- [x] Generate typed scalar/span adapters from ABI signatures.
- [x] Keep generic object, callback, and nested-array marshaling only when required.
- [x] Specialize synchronous entry scheduling and include asynchronous machinery only when used.
- [x] Preserve custom imports, callbacks, cancellation, worker bootstrap, and explicit export inspection.
- [x] Snapshot deferred socket/process inputs before returning from their synchronous start calls.
- [x] Reject run observers when an asynchronous guest poll traps.
- [x] Measure the full representative deployment matrix against the original revision.

## Phase 3 — portable code and WebGPU

- [x] Generate numeric WebGPU binding metadata in Dream from Slang reflection.
- [x] Delete runtime WGSL binding discovery and reject unsupported reflected shapes.
- [x] Share the C shader-bundle reader between native and browser execution.
- [x] Link browser parsing into the main GPU WASM; ship no extra GPU parser module.
- [x] Use typed spans for GPU uploads and command streams.
- [x] Preserve per-reference SHA-256 verification, device-loss handling, and asynchronous cleanup.
- [x] Keep the device alive while child GPU resources still own it, matching native ownership.
- [x] Separate CSV and compression modules and preload only live codec families.
- [x] Copy live companions into aliases and remove stale codec companions on rebuild.
- [x] Run real browser compute/readback and rendering assertions.

Portable runtime primitives and shader parsing are C. Shader reflection/layout processing
is Dream. Codec capabilities retain the existing mature Rust CSV and compression libraries;
they are separate from the C guest runtime. Browser object creation and WebGPU queue calls
remain JavaScript because they invoke browser APIs.

## Phase 4 — standalone WASI

- [x] Separate browser `wasm32-unknown-unknown` from explicit `wasm32-wasip2` and `wasm32-wasip3` environments.
- [x] Pin WASI SDK 34.0, wit-bindgen 0.63.0, Wasmtime 49.0.4, and WIT 0.2.12/0.3.0.
- [x] Generate C HTTP bindings and link executable command components with the SDK.
- [x] Use standard WASI services for stdio, filesystem, clocks, randomness, HTTP, TCP, and UDP.
- [x] Share portable C units while excluding the browser's WASI-named libc adapters.
- [x] Link package C sources and generated C ABI shims against the selected component SDK.
- [x] Keep delays on Dream's timer queue and use p2 pollables/p3 futures and streams in transport adapters.
- [x] Reject unavailable JS/window/GPU/process capabilities and exported component libraries before backend emission.
- [x] Keep CLI parsing native-only on component targets.
- [x] Qualify both previews with real C ABI calls, timers, filesystem, HTTP, TCP/UDP echo, UTF-8, EOF, and denied filesystem access.
- [x] Compare freshly emitted component WASM, WAT, and ABI metadata for deterministic output.

See [Standalone WASI commands](../language/interop/wasi.md). Component networking currently
waits synchronously within a Dream poll; it does not yet interleave pending network work
with other Dream tasks. Command components are experimental.

## Completion checks

- [x] Workspace build.
- [x] Strict Clippy (`--workspace --all-targets -- -D warnings`).
- [x] Workspace tests.
- [x] Full native and Node assertion corpora.
- [x] Documentation tooling tests, API inventory, and strict MkDocs build.
- [x] Generated capability/header, codec-module, C parser, and WIT-binding freshness checks.
- [x] Fresh Hello World, CSV, and GPU compute builds produce byte-identical WASM, WAT, ABI metadata, JavaScript, and required companions.
- [x] Real browser WebGPU and standalone WASI execution.
- [x] Final refresh of recorded size reports and checks after the last edits.

## Measurements and reproduction

`tests/size/` covers Hello World, async entry, typed arrays, callbacks, dynamic JS,
workers, files, HTTP, CSV, compression, GPU compute, and GPU rendering.
The comparison revision is `617305d5`. An isolated checkout built the same fixture sources.
Main WASM ABI sections count toward the download; readable WAT and ABI sidecars do not.

Recorded results are in `tests/size/deployment-sizes.json`. Sizes below are bytes for the complete deployment.

| Fixture | Baseline gzip | Current gzip | Baseline Brotli | Current Brotli |
|---|---:|---:|---:|---:|
| hello | 10,048 | 7,283 | 9,088 | 6,583 |
| async | 15,995 | 14,464 | 14,264 | 12,914 |
| typed_array | 10,129 | 7,846 | 9,157 | 7,053 |
| callback | 10,148 | 8,263 | 9,175 | 7,492 |
| dynamic | 11,324 | 10,874 | 10,172 | 9,738 |
| worker | 15,539 | 14,492 | 13,969 | 12,962 |
| filesystem | 15,348 | 13,303 | 13,809 | 11,940 |
| http | 46,339 | 45,235 | 37,925 | 36,901 |
| csv | 161,334 | 81,695 | 140,216 | 70,996 |
| compression | 163,643 | 112,346 | 141,730 | 96,951 |
| gpu_compute | 37,070 | 36,957 | 31,607 | 31,577 |
| gpu_render | 38,105 | 37,960 | 32,366 | 32,302 |

```sh
dream --release --web -o target/size/hello.wat tests/size/hello.dream
node scripts/check-wasm-size.mjs \
    --fixture hello=target/size/hello.wasm \
    --fixture hello=target/size/hello.web.runtime.js \
    --output target/size/hello.json
```

Include `<stem>.csv.wasm` or `<stem>.compression.wasm` for the corresponding codec family.
List each required artifact once. `--baseline previous.json` rejects gzip or Brotli growth.
The standalone C parser binary under `tests/fixtures/` is a test harness, not a deployment dependency.

Regeneration checks:

```sh
python scripts/generate-capabilities.py --check
python scripts/build-data-codecs.py --check
python scripts/build-gpu-parser.py --check
python scripts/generate-wasi-bindings.py --check
node scripts/bundle-runtime.mjs --check
node --test scripts/test-typed-host.mjs scripts/test-socket-spans.mjs
python scripts/test-webgpu-browser.py
python scripts/qualify-wasi.py
```

The browser qualification expects compiled `gpu_compute` and `gpu_render` fixtures in
`target/minimal-wasm-size/`. The WASI qualifier requires the pinned SDK and engine.

## Follow-up work

- [ ] Integrate nonblocking WASI networking with Dream scheduling and in-flight cancellation.
- [ ] Consolidate platform-independent GPU command validation and uniform packing in C.
- [ ] Replace remaining operation-specific filesystem/network JSON envelopes with generated records where complete-download measurements justify it.
- [ ] Add WIT-defined exported component libraries and additional standalone capabilities.
