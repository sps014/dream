# Freestanding C guest runtime

Compile with `scripts/build-runtime.sh`. **macOS and Linux install steps** (wasi-sdk 33, `wasm2wat`) are in [../README.md](../README.md). Not part of `cargo build`.

Modules are declared once in [`../modules.rs`](../modules.rs) (`RuntimeModule`: extract vs link, sources, `-D`, includes, exports, stack/`--global-base`). The shell script has no per-library `if regex` branch.

## Do

- One concern per `.c` file; share numeric ABI via `include/dream_abi.h`. Extract units import via `include/dream_rt.h`. Portable wrappers (wasm + native) include `include/dream_guest.h`.
- `EXPORT("malloc")` (etc.) must match names in `abi.rs` / host / existing WAT.
- Keep helpers `static`. Locals are scalars only (extract modules).
- Cross-file calls are ordinary prototypes (`rt_malloc` → WASM `$malloc` after splice).
- Extract: compile per `.c` (`--allow-undefined`). Dream’s module provides `$malloc`, `$print_string`, globals.
- Link: `wasm-ld --import-memory` with catalog `wrap` / `exports` / `global_base` / `stack_size`.
- Keep `$__alloc_lock_*` in handwritten WAT unless atomics in C are proven (`i32.atomic.*`).
- If `--release` goldens regress, leave that function’s WAT handwritten.

## Don't

- Do not run clang from `dream` or default CI (`cargo test --workspace`, `windows-latest` release jobs).
- Do not emit a second WASM module, `(memory)`, data segments, or C `static` mutable globals / constructors (`__wasm_call_ctors`) from **extract** units.
- Do not use libc, `memcpy`, VLAs, or `&` on locals in extract units (extract script rejects `$__stack_pointer` / `memcpy`). Link units may use libc via catalog `wrap`.
- Do not rename `$malloc` / `$retain` / `$print_string`.
- Do not hand-edit `generated/*.wat` or promoted `../strings.wat` / `../object.wat` / `../format.wat` / `../regex.wat`; edit `.c` (and `pcre2/` for regex) then re-run `scripts/build-runtime.sh`. Handwritten `../allocator.wat` (placeholders) and `../sync.wat` / `../async.wat` stay emit source until replaced.
- Do not use clang `-O4` / wasm-opt `-O4`.
- Native-only files (heap/host/SIMD) live in [`native/`](native/). Shared wrappers live next to extract C and compile twice (`-DDREAM_NATIVE` on the host path).
