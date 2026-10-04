# WebAssembly system services

This layer manages the memory and system services a Dream WebAssembly program needs. Keep shared scheduling in `../shared/` and portable reference counting and strings in `../../core/`.

## Memory and synchronization

`heap_memory.c` grows linear memory and owns shared heap metadata. Worker instances reapply data segments, so that metadata lives beyond `__heap_base`. `heap.c` manages size classes and free lists for this memory representation.

Allocation locks use the embedding platform table. If memory cannot grow, the runtime reports a panic through the same table without allocating. `sync.c` handles locks and semaphores for ordinary and shared-memory guests. A single-threaded wait that cannot finish reports a panic instead of hanging. `g0.c` and `g0.s` manage per-instance globals.

## External code and callbacks

`allocation.c` provides the allocation entry points needed by the scheduler and bundled C libraries. It uses the Dream heap and does not implement printf. `callback.c` tracks callback ownership by guest instance. `interop_libc.c` connects the pinned WASI library's output and empty-environment calls to Dream platform services.

## Default platform table

`platform.c` supplies allocation, abort, text-output, and lock callbacks. Shared scheduling is in `../shared/async.c`. See the [runtime layer guide](../../README.md) for the embedding contract and failure checks.
