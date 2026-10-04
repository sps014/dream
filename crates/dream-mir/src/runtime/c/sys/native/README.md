# Native system services

This layer connects the runtime to Windows and POSIX services. Put platform-dependent behavior here and keep portable allocation and reference-counting logic in `../../core/`.

It owns mapped pages, platform locks, standard streams, files, environment variables, clocks, callbacks, and workers. The platforms share the `include/dream_thread.h` abstraction. Scheduling common to native and WebAssembly lives in `../shared/`.

`platform.c` defines the default embedding table. `heap_debug.c` and `leak_report.c` handle leak-report formatting, keeping standard I/O and environment access outside the core layer.

`../../../modules.rs` registers these units. The pinned Clang builds them into runtime bitcode. Regular expressions and host capabilities keep their own registries.

Run `scripts/bench-runtime.sh` from the repository root for runtime benchmarks. Read the [runtime layer guide](../../README.md) before changing an embedding callback or allocation contract.
