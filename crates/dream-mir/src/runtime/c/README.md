# Freestanding C guest runtime

Compile with `scripts/build-runtime.sh`. **macOS and Linux install steps** (wasi-sdk 33, `wasm2wat`) are in [../README.md](../README.md). Not part of `cargo build`.

## Do

- One concern per `.c` file; share ABI via `include/dream_abi.h` and imports via `include/dream_rt.h`.
- `EXPORT("malloc")` (etc.) must match names in `abi.rs` / host / existing WAT.
- Keep helpers `static`. Locals are scalars only.
- Cross-file calls are ordinary prototypes (`rt_malloc` → WASM `$malloc` after splice).
- Compile per `.c` (`--allow-undefined`). Dream’s module provides `$malloc`, `$print_string`, globals.
- Keep `$__alloc_lock_*` in handwritten WAT unless atomics in C are proven (`i32.atomic.*`).
- If `--release` goldens regress, leave that function’s WAT handwritten.

## Don't

- Do not run clang from `dream` or default CI (`cargo test --workspace`, `windows-latest` release jobs).
- Do not emit a second WASM module, `(memory)`, data segments, or C `static` mutable globals / constructors (`__wasm_call_ctors`).
- Do not use libc, `memcpy`, VLAs, or `&` on locals (extract script rejects `$__stack_pointer` / `memcpy`).
- Do not rename `$malloc` / `$retain` / `$print_string`.
- Do not hand-edit `generated/*.wat` or promoted `../strings.wat` / `../object.wat` / `../format.wat` / `../regex.wat`; edit `.c` (and `pcre2/` for regex) then re-run `scripts/build-runtime.sh`. Handwritten `../allocator.wat` (placeholders) and `../sync.wat` / `../async.wat` stay emit source until replaced.
- Do not use clang `-O4` / wasm-opt `-O4`.
- Native host C (real pointers / memcpy / mmap) lives in [`native/`](native/), not this wasm extract tree.
