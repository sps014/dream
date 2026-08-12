# Microbench baseline — tiered GC

Recorded with `dream --release run tests/bench/microbenches.dream` after the ARC→GC
cutover (`docs/compiler/12-tiered-gc.md`). Absolute values vary by host — use relative
deltas. Re-run via `./scripts/run-microbenches.sh`.

## Snapshot (2026-08-12, native wasmtime)

Nursery = 1 MiB. `$malloc` is the nursery bump (no extra `$gc_alloc` call). Scratch
locals are declared only when the MIR uses them. Young dests skip `$write_barrier`.

| Bench | ns_per_op | notes |
|-------|----------:|-------|
| list_push | 6 | |
| list_clear_reuse | 6 | ARC-era ~9 |
| string_eq | 11 | |
| alloc_churn | 23 | |
| map_clear_reuse | 36 | ARC-era ~46 |
| scratch_arena | 59 | |
| gc_locals | 57 | |
| map_get_set | 81 | noisy vs prior ~64 |
| list_insert_mid | 91 | ARC-era ~96 |
| string_concat | 112 | |
| substring | 131 | ARC-era ~127 |
| string_builder | 395 | |
| char_scan | 4.1k | |
| regex_find | 31k | |

`list_clear_reuse` beats the ARC-era figure on this host. `map_clear_reuse` stays ahead
of ARC (~36 vs ~46).

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
