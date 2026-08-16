# 13 — LLVM WIP status (continue here)

Snapshot: **2026-08-16**. Branch: **`llvm`** (uncommitted work lives here). Design: [12 — LLVM backend](./12-llvm-backend.md).

`dream run` = MIR → LLVM IR text → clang + `dream-rt` → native `.out`. WAT/relooper is deleted. Wasmtime is **not** the default runner; it remains only for wasm32 DAP / playground-style `wasm_runner` + `src/execution/host/`.

When picking this up: read this file, then `native_c_sym` in `crates/dream-llvm/src/emit/names.rs`, then the matching wasmtime ABI in `src/execution/host/*.rs`.

## Done

- Crates: `dream-llvm` (emit + clang), `dream-rt` (C heap + hosts).
- Guest pointers stay `i32` offsets into `dream-rt` linear heap (same ARC header as MIR ABI).
- Emit split: `crates/dream-llvm/src/emit/` (`mod`, `names`, `function`, `stmt`, `ops`, `place`, `call`, `intern`, `protocol`).
- Unmapped `@js` stubs call `dream_unimplemented("<field>")`, not a hardcoded `"js"`.
- Native hosts in `crates/dream-rt/c/dream_host.c` (linked from `clang.rs` with `-lm`):
  - **math** (`env` sin/cos/…/pow)
  - **time** (`timeNowNanos`, `dateNowMillis`, `dateLocalOffsetMinutes`, `delayMs` as blocking `nanosleep`)
  - **files** (read/write/append, bytes, exists/delete/size/isDir, dir list/create)
  - **process** (platform, os family, cwd/setCwd, args, env get/set tagged `"1"+value`, exe path, exit, readLine/readKey)
- Smoke: `tests/cases/math_advanced.dream`, `platform_basic.dream` run natively.
- E2E skips `webworker*` on native. DAP / `wasm_opt_test` skip without clang wasm32 + `wasm-ld`.
- `dream_release` drop depth cap 64. Generated `dream_drop` can still blow the stack.

## Wasmtime we had vs native now

Source of truth for the old ABI: `src/execution/host/{file,process,crypto,text,datetime,http,net,gpu,worker,webview,console}.rs`. Port by copying **wire formats** (string intern, `byte[]` = i32 len prefix + payload, processRun header), not by calling wasmtime.

| Area | Wasmtime (`src/execution/host`) | Native (`dream_host.c` / `native_c_sym`) |
|------|----------------------------------|------------------------------------------|
| Math | `env` sin…round | done |
| Clock / local TZ | chrono + chrono-tz + iana-time-zone | local offset via `tm_gmtoff`; **IANA names → `-999999`** except UTC/GMT |
| Files (path) | std::fs | done (including `fileReadBytes` / `fileWriteBytes`) |
| File **handles** | `fileOpen`, `fileHandleRead/Write/Seek/Close` | **missing** |
| Process meta | platform/cwd/env/args/exe | done (`processArgs` skips argv[0], join `\n`) |
| Process **child** | `processRun/Spawn`, stdin/stream/wait/kill | **missing** |
| Console | crossterm raw `readKey` | `getchar` / `fgets` (no raw TTY) |
| Unicode | `unicodeNormalize/ToLower/ToUpper/Graphemes` | **missing** |
| Crypto | sha256/512, HMAC, AES-GCM, CSPRNG | **missing** |
| HTTP / TCP / WS | reqwest + sockets | **missing** (trap) |
| GPU / webview | wgpu / wry | **missing by design** (trap) |
| `js` type / JS object API | trap in wasmtime; real in `runtime/dream.js` | **trap by design** |
| WebWorker | shared memory + wasmtime threads | **WASM/JS-only by design** |
| C FFI | `dream_ffi` / `c-ffi` feature | **missing** |
| DAP | wasmtime + `dream_debug` imports | wasm32 LLVM + wasm-ld; Apple clang often **no wasm32** |

Do **not** implement a JS engine on native. `jsGetV`, `workerSpawn`, `gpu*`, `http*`, `webview*` should keep trapping with the field name.

## Known bugs (fix first)

1. **i32 stored as i64** — `store_width` in `emit/place.rs` emits `dream_store_i64(..., i64 %t)` when the SSA value is still `i32`. Microbench failed: `microbenches.ll` `%t41` type mismatch. Coerce (`sext`/`zext`) from `op_ty` / `llvm_val_ty` of the value, not only the slot type. Same risk on float/double stores.
2. **`regex_find` SIGSEGV** in `dream_drop` (recursive drop). Cap or iterate generated drop the same way `dream_release` caps depth 64 — or fix the walk if it is a bad pointer, not just recursion.
3. **Timezone goldens** need IANA (wasmtime used `chrono-tz`). C currently returns `UNKNOWN_ZONE_OFFSET` for `America/New_York`.
4. **`delayMs`** is an **async** stdlib import; native C blocks the thread. Wasmtime queued a timer. Scheduler/async tests may disagree.
5. Leftover MIR GVN/LICM/unroll still in `PassManager::default_pipeline()` — LLVM `-O` already does this. Do not add more; trimming is optional.

## Tests / benches last seen

- `cargo clippy -p dream-llvm -p dream-rt --all-targets -- -D warnings` green after emit split.
- Ignored suite log `/tmp/dream-ignored.txt`: DAP 3 passed, then `run_all_e2e_cases` still running when the log stopped (pipeline used `tail`, exit code untrustworthy). Re-run: `cargo test --workspace -- --ignored --test-threads=1 --no-fail-fast`.
- `tests/bench/out/native.txt`: substring 111, list_insert_mid **150** (baseline ~96), list_clear_reuse **15** (~9), map_clear_reuse 33, scratch_arena 34; `string_eq` 0 (folded); regex crashed.
- `tests/bench/out/llvm-native.txt`: clang i64 store error (bug 1).
- Likely remaining goldens once hosts exist: `@json` generators, regex, IANA TZ, uint/`byte[]` print, RC probes (`gc_complete`, `value_union_option`), `html_syntax_dsl` / `quote_syntax_dsl`.

## Next work (order)

1. Coerce in `store_width` / `load_width`; re-run `dream --release run tests/bench/microbenches.dream`.
2. Fix `dream_drop` crash; finish regex bench.
3. Port file handles + process child from wasmtime wire format (`process.rs` comments document headers).
4. Crypto + unicode in C (or a small Rust cdylib — prefer C/`libcrypto` only if already a dep; wasmtime used `sha2`/`aes-gcm`/`getrandom`).
5. IANA TZ: either ship a tiny zone table or link a TZ library; match `UNKNOWN_ZONE_OFFSET = -999999`.
6. Full ignored e2e; print `{e}` on ICE (check `tests/e2e_tests.rs`).
7. Wasm32: clang wasm-ld + `dream_debug` imports; DAP currently skipped on many Macs.
8. Optional: split `emit/protocol.rs` / `call.rs` further (still >400 LOC); drop WAT-era MIR scalar passes.

## Layout cheat sheet

```
crates/dream-llvm/src/emit/     IR text
crates/dream-llvm/src/clang.rs  clang driver (-lm, skip entry.c on wasm)
crates/dream-rt/c/dream_rt.c    heap, print, strings, SIMD, unimplemented
crates/dream-rt/c/dream_host.c  OS hosts
crates/dream-rt/c/entry.c       main → dream_rt_set_args + dream_user_main
src/execution/native_runner.rs  run the .out
src/execution/host/             wasmtime ABI to copy, not to call from native
```
