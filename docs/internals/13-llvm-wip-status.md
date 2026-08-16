# 13 — LLVM WIP status (continue here)

Snapshot: **2026-08-16**. Branch: **`llvm`**. Design: [12 — LLVM backend](./12-llvm-backend.md).

## Why the heap is C and hosts are Rust

Guest pointers are **`i32` offsets** into a linear heap (same ARC header as MIR/WASM). That heap must compile **freestanding for wasm32** with clang (`-nostdlib`). Rust `std` cannot do that job, so `dream_rt.c` stays C.

Wasmtime hosts lived in the **compiler process** (`src/execution/host/`). Native LLVM puts those in the **user binary**. Native hosts are **`#[no_mangle] extern "C"` in `crates/dream-rt/src/host/`**, linked as `libdream_rt.a`.

`js` never had a wasmtime engine: native is a **compile-time error**. GPU used wgpu **inside the CLI**; LLVM-native now links the same logic into `libdream_rt.a`.

`dream run` = IR + `entry.c` + `libdream_rt.a` + system frameworks.

## Hosts vs wasmtime

| Area | Native now |
|------|------------|
| Math, files (path+bytes), process meta, console, delay | C `dream_host.c` |
| IANA TZ, monotonic/wall clock | Rust `host/datetime.rs` |
| File handles | Rust `host/file_handle.rs` |
| Process run/spawn/stdio/wait/kill | Rust `host/process.rs` |
| Unicode | Rust `host/text.rs` |
| Crypto | Rust `host/crypto.rs` |
| HTTP + stream | Rust `host/http.rs` |
| TCP + WebSocket | Rust `host/net.rs` |
| GPU compute/render/window | Rust `host/gpu/` (wgpu/winit; same logic as wasmtime) |
| webview | Rust `host/webview.rs` (wry, in-process) |
| WebWorker | Rust `host/worker.rs` (OS threads, shared C heap) |
| `@c` | Rust `host/c_ffi.rs` (libloading + libffi) |
| `js*` | compile-time error on native |

LLVM `Await` stores the host return value as the result (no Future unwrap). Async hosts return the **payload** (`char[]` / error code / `i64` timestamp), matching that lowering.

## Still broken / next

1. `@shared` atomics across worker threads (heap mutex covers malloc, not field stores).
2. Native DAP (clang `-g` only; DAP still wasmtime).
3. Full ignored e2e corpus as the default gate.
4. Debug `libdream_rt.a` is large (reqwest + wgpu + wry).

## Layout

```
crates/dream-rt/c/dream_rt.c      linear heap (C, wasm32+native)
crates/dream-rt/c/dream_host.c    math/files/process meta (C)
crates/dream-rt/src/host/         wasmtime-parity OS hosts (Rust)
crates/dream-rt/src/host/gpu/     wgpu (shared with wasmtime linker)
crates/dream-rt/src/host/worker.rs WebWorker OS threads
crates/dream-rt/src/host/webview.rs wry
crates/dream-rt/src/host/c_ffi.rs  @c libffi
crates/dream-rt/src/guest.rs      read/write guest strings/byte[]
crates/dream-llvm/src/clang.rs    native: link native_archive(); wasm: compile C only
src/execution/host/gpu/mod.rs     wasmtime linker only
```
