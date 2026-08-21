# Guest runtime C sources

The guest runtime for every target is **C** under this directory. There is no WAT runtime anymore.

- **wasm32 guest** (`dream --wasm` / `--web` / `--node`): [`wasm32/`](wasm32/) (heap, libc, g0, sync/weak stubs) plus shared units from [`native/`](native/) (strings, object, format, panic, closure, async, defer, simd, ffi). `dream_mir::runtime::wasm32_runtime_c_files()` is the list; wasi-sdk clang compiles them (see `src/driver/c_wasm32.rs`).
- **Native hosts** (`dream run`): [`native/`](native/) (`uintptr_t`, mmap size-class heap, platform SIMD width) via `native_runtime_units()`.
- **Linked libraries** (PCRE2 regex): [`regex.c`](regex.c) + [`pcre2/`](pcre2/), compiled per target from the catalog in `crates/dream-mir/src/runtime/modules.rs` when `RuntimeNeed::REGEX` is set.

`TAG_*` / heap offsets / `DREAM_REGEX_*` live in [`include/dream_abi.h`](include/dream_abi.h) and
[`../../abi.rs`](../../abi.rs) (lockstep test `dream_abi_h_matches_abi_rs`).

## Native C (host `zig cc` / clang)

Default `dream run` compiles the native runtime. Without a system `cc`:

```bash
dreamer toolchain install cc    # pinned Zig → zig cc
```

See [`native/README.md`](native/README.md). For wasm32 compilation install wasi-sdk:

```bash
dreamer toolchain install wasi-sdk
```
