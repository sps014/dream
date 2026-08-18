# 14 — Perf backends: Wasm first (LLVM branch is not the path)

## Status

Branch **`llvm`** was an experiment that **deleted WAT emit**, shipped a textual LLVM IR +
`dream-rt` stack, and then stalled. On that tip, microbenches were **slower than current
Wasm/wasmtime on `main`** for most rows (e.g. `arc_locals` ~166 vs ~44, `string_concat` ~212 vs
~50, `alloc_churn` ~46 vs ~21, `scratch_arena` ~43 vs ~5), with IR typing bugs (`dream_store_i64`),
drop/RC crashes, and other host gaps. **Do not merge or “revive” that branch as the perf plan.**

A worktree at `../Dream-llvm` may still exist for archaeology only.

## Active plan

1. **Wasm hotpath on `main`** — keep WAT → wasmtime; grind list/map/string/allocator without
   benchmax. Honest `regex_find` stays `[a-z]+\d+` (Pike VM).
2. **Native codegen** — MIR → C lives in [`crates/dream-mir/src/backend/c`](../../crates/dream-mir/src/backend/c)
   with host runtime [`crates/dream-mir/src/runtime/c/native`](../../crates/dream-mir/src/runtime/c/native)
   (`uintptr_t`, `memcpy`, mmap heap, platform SIMD, Pike computed goto). `dream run` stays
   WAT → wasmtime until `scripts/bench-native-c.sh` and `microbenches.dream` beat `--release` wasm.
   `Target::NativeC` is opt-in; do not merge branch `llvm`.

Opt-in: `dream --native-c file.dream` or `dream --backend c file.dream` writes `.c`.
`dream run --backend c` compiles with `cc` and execs. Default `dream run` is still
WAT → Wasmtime. Web/node stay WAT + JS (`--runtime --web` is rejected with `--native-c`).

Until then, “C#-like ns on scan / substring / regex” is acknowledged as a **substrate floor** under
wasmtime, not something the current `llvm` branch fixes.

## Non-goals

- Merging `llvm` tip into `main`.
- Benchmax (`\d+`-only regex scoreboard).
- Claiming native parity while native is slower than Wasm.
