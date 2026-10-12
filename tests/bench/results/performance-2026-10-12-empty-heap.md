# Heap-free reporting reachability, 2026-10-12

The targeted cost is linking heap diagnostics into scalar programs solely for exit-time reports.
A conservative MIR proof permits a zero-count report for scalar code and direct calls within the
analyzed module. Managed/aggregate storage, async, intrinsics, unknown operations, FFI and exports
retain full heap reporting. Numeric print paths that construct temporary strings retain actual
allocation counts. Entry parameters retain heap services for generated argument-array glue.

The internal reporting ABI now takes the heap requirement; all call sites migrate together.
Debug reporting and Release `DREAM_DEBUG_LEAKS` activation produce the same report as the reference.
The dedicated harness ensures zero-heap reporting never invokes heap state readers.

On local Apple arm64, empty-program symbols drop from 19 to 11 and its text section from 1,628
bytes to 104 bytes. The binary drops from 50,296 to 49,896 bytes. Literal print also shrinks;
argument and long-formatting compute programs preserve their heap machinery. All outputs and
forced leak reports match their preserved compiler/runtime references. The 20-round launch
measurement is pending; this record makes no execution speedup claim.

This adds a narrow proof for diagnostic roots. Broader runtime-unit dependency selection and
cross-platform hardware acceptance remain outstanding.
