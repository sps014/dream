# Guest runtime C sources

The guest runtime for every target is **C** under this directory. The driver compiles it to LLVM
bitcode and links it with the generated module before `opt` (see
[`docs/internals/06-llvm-backend.md`](../../../../docs/internals/06-llvm-backend.md)).

- **wasm32 guest** (`dream --wasm` / `--web` / `--node`): [`wasm32/`](wasm32/) (heap, libc, g0, sync/weak stubs) plus shared units from [`native/`](native/) (strings, object, format, panic, closure, async, defer, simd, ffi). `dream_mir::runtime::wasm32_runtime_c_files()` is the list; wasi-sdk clang compiles them to bitcode (see `src/execution/llvm/wasm.rs` and `src/driver/wasi.rs`).
- **Native hosts** (`dream run`): [`native/`](native/) (`uintptr_t`, mmap size-class heap, platform SIMD width) via `native_runtime_units()`, compiled to bitcode by the pinned clang (`src/execution/llvm/runtime.rs`). [`native/llvm_inline.c`](native/llvm_inline.c) holds the hot helpers the optimizer should inline.
- **Linked libraries** (PCRE2 regex): [`regex.c`](regex.c) + [`pcre2/`](pcre2/), compiled per target from the catalog in `crates/dream-mir/src/runtime/modules.rs` when `RuntimeNeed::REGEX` is set.

`TAG_*` / heap offsets / `DREAM_REGEX_*` live in [`include/dream_abi.h`](include/dream_abi.h) and
[`../../abi.rs`](../../abi.rs) (lockstep test `dream_abi_h_matches_abi_rs`).

## Toolchains

Every build needs the pinned LLVM; native builds also need a linker driver (`cc`):

```bash
dreamer toolchain install llvm  # pinned LLVM (clang, llvm-link, opt, llc)
dreamer toolchain install cc    # pinned Zig → zig cc, when there is no system cc
```

See [`native/README.md`](native/README.md). For wasm32 compilation install wasi-sdk:

```bash
dreamer toolchain install wasi-sdk
```
