# Guest C (linked libs + native)

Same-module WASM helpers are **WAT** (`../allocator.wat`, `../strings.wat`, …). This directory is:

- **Linked wasm32 libraries** — today PCRE2 (`regex.c`, `regex_wasm_libc.c`, `pcre2/`). Rebuild with `scripts/build-runtime.sh`.
- **Native host runtime** — [`native/`](native/) for `dream run`.

Modules are declared in [`../modules.rs`](../modules.rs). The shell script compiles catalog `wasm_c` with `wasm-ld` (`wrap` / `exports` / `global_base` / `stack_size`) and writes `wat_out`. It does not extract or rewrite helper bodies.

## Do

- Share numeric ABI via `include/dream_abi.h`. Portable wrappers (wasm regex + native) include `include/dream_guest.h`.
- Link: `wasm-ld --import-memory` with catalog `wrap` / `exports` / `global_base` / `stack_size`.
- Native-only files live in [`native/`](native/).

## Don't

- Do not run clang from `dream` or default CI (`cargo test --workspace`, `windows-latest` release jobs).
- Do not hand-edit `../regex.wat`; edit `regex.c` / `pcre2/` then `scripts/build-runtime.sh`.
- Do not add a guest-C extract pipeline for `$malloc` / strings / panic. Author those as WAT.
- Do not use clang `-O4` / wasm-opt `-O4`.
