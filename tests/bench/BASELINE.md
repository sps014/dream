# Microbench baseline — tiered GC

Recorded with `dream --release run tests/bench/microbenches.dream` after the ARC→GC
cutover (`docs/compiler/12-tiered-gc.md`). Absolute values vary by host — use relative
deltas. Re-run via `./scripts/run-microbenches.sh`.

## Post-Gen0 snapshot (2026-08-12, native wasmtime)

Nursery = 256 KiB. Gen0 bump + evacuate; epoch-gated root reload; no call-arg spill
(operands cannot allocate); unmanaged `List`/`Map`/`Set.clear` skips zeroing traced
slots.

| Bench | ns_per_op | notes |
|-------|----------:|-------|
| string_eq | 11 | |
| list_push | 16 | |
| list_clear_reuse | 18 | ARC-era ~9 |
| alloc_churn | 25 | |
| map_clear_reuse | 56 | ARC-era ~46 |
| gc_locals | 57 | |
| scratch_arena | 74 | |
| string_concat | 99 | |
| map_get_set | 101 | |
| list_insert_mid | 112 | ARC-era ~96 |
| substring | 128 | ARC-era ~127 |
| string_builder | 284 | |
| char_scan | 4.1k | |
| regex_find | 46k | |

`substring` matches ARC. `map_clear_reuse` is within ~20%. `list_clear_reuse` still
pays per-`push` root prologue / epoch checks that ARC elided on `int` elements.

### ARC-era reference (historical)

| Bench | ns_per_op (approx) | notes |
|-------|-------------------:|-------|
| list_clear_reuse | ~9 | ARC + clear reuse |
| map_clear_reuse | ~46 | |
| list_insert_mid | ~96 | |
| substring | ~127 | |

```bash
./scripts/run-microbenches.sh
# or
dream --release run tests/bench/microbenches.dream
```
