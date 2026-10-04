# WASI and JS system services

`heap_memory.c` owns linear-memory page growth and shared heap metadata. Worker instances
reapply data segments, so the metadata lives beyond `__heap_base`. `heap.c` owns size-class
and free-list allocation for this representation. Allocation locks use the platform table.
Failed page growth reports an allocation-free panic through that same table.

`allocation.c` supplies the libc allocation entry points needed by the scheduler and vendored
C libraries, using the Dream heap. It contains no printf implementation. `sync.c` implements
locks and semaphores for both single-threaded and shared-memory guests; single-threaded waits
that cannot complete report a panic. `g0.c` and `g0.s` own per-instance globals.

`callback.c` tracks C callback ownership by guest instance. `interop_libc.c` bridges the pinned
WASI libc stdout/stderr and empty-environment syscalls to Dream platform services.

`platform.c` supplies the default allocation, abort, encoded text-output and lock callbacks.
Shared scheduling lives in `../shared/async.c`; portable ARC and strings live in `../../core/`.
