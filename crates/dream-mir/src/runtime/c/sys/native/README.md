# Native system services

POSIX and Win32 services share the existing `include/dream_thread.h` abstraction. This
layer owns OS page maps, platform locks, stdio, filesystem, environment, time, callbacks,
workers. Shared async scheduling lives in `../shared/`. Portable allocation and ARC logic live in `../../core/`.
The default embedding platform table is defined in `platform.c`; leak formatting lives in
`heap_debug.c` and `leak_report.c`, keeping stdio/getenv out of core.

Use `scripts/bench-runtime.sh` for runtime microbenchmarks. `../../../modules.rs` registers
native sys units alongside the shared core; runtime sources compile to LLVM bitcode with
the pinned clang. Regex/host capability libraries keep their separate registries.
