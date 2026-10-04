# Runtime sources

This directory contains the C code that supports running Dream programs on each target. Use this guide when changing the runtime or the way a program is built. For writing Dream programs, start with the [user documentation](../../../../docs/index.md).

## Find the right layer

| Location | Responsibility |
| --- | --- |
| `c/core/` | Portable allocation, strings, weak references, regions, and panic handling |
| `c/sys/native/` | Windows and POSIX system services |
| `c/sys/wasi/` | WebAssembly memory and browser/Node service adapters |
| `c/sys/shared/` | Scheduling used by both native and WebAssembly builds |
| `c/regex.c` and `c/pcre2/` | Optional regular-expression support |

`modules.rs` selects the units for each target. Native networking, GPU, and window capabilities remain separate `dream-host-*` crates. Do not add a second runtime implementation for another output format.

The build driver compiles these units with the pinned Clang and combines their bitcode with the program before optimization. Read the [backend handbook](../../../../docs/internals/06-llvm-backend.md) before changing this boundary.

## Shared definitions

Keep `c/include/dream_abi.h` and `../abi.rs` in sync. The `dream_abi_h_matches_abi_rs` test checks shared numeric constants. Runtime call signatures come from the compiled runtime itself.

## Required tools

```sh
dreamer toolchain install llvm
dreamer toolchain install cc
```

Every build needs the pinned LLVM tools. Native output also needs a suitable linker. WebAssembly output needs its target headers and libraries. See [Toolchain setup](../../../../docs/reference/tooling/toolchain.md).

Read [the core runtime guide](c/README.md) for the embedding table and freestanding checks.
