# Microbench baseline — tiered GC

Recorded with `dream --release run tests/bench/microbenches.dream` after the ARC→GC
cutover (`docs/compiler/12-tiered-gc.md`). Absolute values vary by host — use relative
deltas. Re-run via `./scripts/run-microbenches.sh`.

## Snapshot (2026-08-12, native wasmtime)

Nursery = 1 MiB. Cached GC bounds in WASM globals; young dests skip `$write_barrier`;
alloc fast path is a bump (no `GC_REQUEST` poll); inlined root-table set/get.

| Bench | ns_per_op | notes |
|-------|----------:|-------|
| list_push | 6 | |
| list_clear_reuse | 8 | ARC-era ~9 |
| string_eq | 10 | |
| alloc_churn | 24 | |
| map_clear_reuse | 34 | ARC-era ~46 |
| gc_locals | 56 | |
| scratch_arena | 62 | |
| map_get_set | 64 | |
| list_insert_mid | 90 | ARC-era ~96 |
| string_concat | 94 | |
| substring | 128 | ARC-era ~127 |
| string_builder | 346 | |
| char_scan | 4.3k | |
| regex_find | 35k | |

`list_clear_reuse` / `substring` / `list_insert_mid` match or beat the ARC-era figures
on this host. `map_clear_reuse` is ahead of ARC (~34 vs ~46).

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
