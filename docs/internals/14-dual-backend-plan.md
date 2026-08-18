# 14 — Dual backend: Wasm (default) and native C (opt-in)

Same MIR, two emitters. There is **no LLVM backend**.

## What ships

| | Wasm (`Target::Wasm`, default) | Native C (`Target::NativeC`) |
|---|---|---|
| Emit | Relooper → WAT/WASM (`backend/wasm`) | MIR → C99 (`backend/c`) |
| Runtime | Same-module `runtime/*.wat`; PCRE2 interpreter in `regex.wat` | `runtime/c/native/` (`uintptr_t`, `memcpy`, mmap heap, platform SIMD); PCRE2-16 **JIT** |
| Run | `dream run` → wasmtime | `dream run --backend c` → `zig cc` or `CC`, then exec |
| Web / Node | `--runtime --web` / `--node` | Rejected (no WASM module) |

Opt-in compile-to-C:

```bash
dream --native-c file.dream              # writes target/debug/*.c, *.o, *.bin
dream --backend c file.dream
dream run --backend c --release file.dream
```

`dream run` with no `--backend` stays WAT → Wasmtime until native C wins
`scripts/bench-native-c.sh` / `microbenches.dream` against `--release` wasm. Do not
switch the default while native is slower.

Numeric ABI (`TAG_*`, string header, future slots) is shared in
`crates/dream-mir/src/abi.rs` and `crates/dream-mir/src/runtime/c/include/dream_abi.h`
(lockstep test `dream_abi_h_matches_abi_rs`).

## Layout

- Wasm helpers: authored WAT under `crates/dream-mir/src/runtime/*.wat`.
- Native helpers: `crates/dream-mir/src/runtime/c/native/`.
- Shared regex wrapper: `crates/dream-mir/src/runtime/c/regex.c` (`-DDREAM_NATIVE` on the host path).
- Catalog (link lists, PCRE2 sources, `--global-base`): `crates/dream-mir/src/runtime/modules.rs`.
- Rebuild `regex.wat` only: `dreamer toolchain install wasi-sdk` then `scripts/build-runtime.sh` (not cargo / Windows CI).

## Non-goals

- LLVM IR, libLLVM, or merging branch `llvm`.
- Deleting the Wasm backend.
- Benchmax (`\d+`-only regex scoreboard).
- Claiming native parity while native is slower than Wasm.
- Generating guest `$malloc` / strings WAT from C (guest core stays WAT).
