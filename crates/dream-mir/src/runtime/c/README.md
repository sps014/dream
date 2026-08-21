# Guest C (wasm32 + native + linked libs)

The guest runtime is **C only** — there is no WAT runtime anymore. This directory holds:

- **wasm32 guest** — [`wasm32/`](wasm32/) (heap, libc, g0, sync/weak stubs) plus shared units from [`native/`](native/). Compiled by wasi-sdk clang (`dreamer toolchain install wasi-sdk`); see `src/driver/c_wasm32.rs`.
- **Native host runtime** — [`native/`](native/) for `dream run`.
- **Linked libraries** — today PCRE2 ([`regex.c`](regex.c), [`regex_wasm_libc.c`](regex_wasm_libc.c), [`pcre2/`](pcre2/)), compiled per target when the catalog (`../modules.rs`) says `RuntimeNeed::REGEX`.

## Do

- Share numeric ABI via `include/dream_abi.h`. Portable wrappers (wasm regex + native) include `include/dream_guest.h`.
- Native-only files live in [`native/`](native/).
- Keep the wasm32 unit list in `../modules.rs` (`WASM32_CORE_C`) in sync with new helper files.

## Don't

- Do not run clang from default CI paths that lack toolchains (the e2e wasm32 tests gate on wasi-sdk presence; `cargo test --workspace` stays green without it).
- Do not add a WAT authoring path for guest helpers; edit the C.
- Do not use clang `-O4` / wasm-opt `-O4`.
