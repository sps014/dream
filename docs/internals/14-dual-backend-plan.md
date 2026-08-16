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
2. **Native codegen** — only if restarted as a **new** design that:
   - keeps WAT as a first-class backend (dual emit, not replace),
   - proves **faster than Wasm** on `microbenches.dream` before any default switch,
   - does not land half-broken IR / drop paths.

Until then, “C#-like ns on scan / substring / regex” is acknowledged as a **substrate floor** under
wasmtime, not something the current `llvm` branch fixes.

## Non-goals

- Merging `llvm` tip into `main`.
- Benchmax (`\d+`-only regex scoreboard).
- Claiming native parity while native is slower than Wasm.
