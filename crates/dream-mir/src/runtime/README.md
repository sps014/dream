# Guest runtime C sources

The guest runtime for every target is **C** under this directory. The driver compiles it to LLVM
bitcode and links it with the generated module before `opt` (see
[`docs/internals/06-llvm-backend.md`](../../../../docs/internals/06-llvm-backend.md)).

- [`c/core/`](c/core/) owns portable heap/ARC, strings, regions, weak references and panic.
  `core/inlines.c` supplies external definitions of the ABI header's inline helpers.
- [`c/sys/native/`](c/sys/native/) owns POSIX/Win32 services and the native platform table.
- [`c/sys/wasi/`](c/sys/wasi/) owns linear-memory pages, allocation bridges, globals,
  synchronization and the WASI/JS platform table.
- [`c/sys/shared/`](c/sys/shared/) owns scheduling shared by native and wasm32.
- [`c/regex.c`](c/regex.c) and vendored [`c/pcre2/`](c/pcre2/) provide optional regex units.
  Native host capabilities remain separate `dream-host-*` crates.

[`modules.rs`](modules.rs) lists units by layer and supplies each target's compile inventory.
Numeric ABI constants live in [`c/include/dream_abi.h`](c/include/dream_abi.h) and
[`../abi.rs`](../abi.rs) (lockstep test `dream_abi_h_matches_abi_rs`).

## Toolchains

Every build needs the pinned LLVM; native builds also need a linker driver (`cc`):

```bash
dreamer toolchain install llvm  # pinned LLVM (clang, llvm-link, opt, llc)
dreamer toolchain install cc    # pinned Zig → zig cc, when there is no system cc
```

See [`c/README.md`](c/README.md) for the embedding platform table and freestanding gate.
The pinned LLVM tools compile native and wasm32 runtime bitcode; wasm builds use the bundled WASI headers.
