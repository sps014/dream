# 14 — Dual backend: native C (default run) and Wasm (web)

Same MIR, two emitters. There is **no LLVM backend**. `dream run` / `test` / `debug-adapter` compile MIR → C → `cc` → `.bin`. WAT/WASM is for `--runtime --web` / `--node` (and explicit `--backend wasm` compile-only).

## What ships

| | Native C (`Target::NativeC`, default run) | Wasm (`Target::Wasm`) |
|---|---|---|
| Emit | MIR → C99 (`backend/c`) | Relooper → WAT/WASM (`backend/wasm`) |
| Runtime | `runtime/c/native/` (`uintptr_t`, `memcpy`, mmap heap, platform SIMD); PCRE2-16 **JIT** | Same-module `runtime/*.wat`; PCRE2 interpreter in `regex.wat` |
| Run | `dream run` → clang/zig cc, then exec the `.bin` | Not used for `dream run` |
| Debug | `#line` + clang `-g` + `lldb-dap` | Browser later; not the CLI DAP |
| Web / Node | Rejected | `--runtime --web` / `--node` |

```bash
dream run file.dream                     # native C .bin
dream --backend wasm file.dream          # compile WAT/WASM only
dream --runtime --web file.dream         # browser artifacts under target/web/
```

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
- Deleting the Wasm backend (browser still needs it).
- c as a native runner.
