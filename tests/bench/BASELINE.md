# Microbench baseline — tiered GC

Recorded with `dream --release run tests/bench/microbenches.dream` after the ARC→GC
cutover (`docs/compiler/12-tiered-gc.md`). Absolute values vary by host — use relative
deltas. Re-run via `./scripts/run-microbenches.sh`.

## Snapshot (2026-08-13, native wasmtime)

Nursery = 1 MiB. Regex Pike-VM reused per `Regex`; SOA thread queues; UTF-8 byte walk;
first-class skip for unanchored CharClass bodies.

| Bench | ns_per_op | notes |
|-------|----------:|-------|
| list_clear_reuse | 8 | |
| list_push | 10 | |
| string_eq | 15 | |
| alloc_churn | 28 | |
| map_clear_reuse | 38 | |
| gc_locals | 73 | |
| scratch_arena | 80 | |
| map_get_set | 90 | |
| string_concat | 115 | |
| list_insert_mid | 121 | |
| substring | 215 | |
| string_builder | 382 | |
| char_scan | 920 | |
| **regex_find** | **9.6k** | was ~36k (VM reuse + skip + byte walk) |

`regex_find` is the large win this pass (~3.7×). Other rows are noise vs the previous snapshot.

### Prior snapshot (2026-08-12, string/remset pass)

| Bench | ns_per_op | notes |
|-------|----------:|-------|
| list_push | 8 | |
| list_clear_reuse | 9 | |
| string_eq | 16 | |
| alloc_churn | 32 | |
| map_clear_reuse | 38 | |
| gc_locals | 78 | |
| scratch_arena | 81 | |
| map_get_set | 89 | |
| list_insert_mid | 115 | |
| string_concat | 142 | |
| substring | 159 | |
| string_builder | 408 | |
| char_scan | 867 | was ~4.1k |
| regex_find | 36k | |

### ARC-era reference (historical)

| Bench | ns_per_op (approx) | notes |
|-------|-------------------:-------|
| list_clear_reuse | ~9 | ARC + clear reuse |
| map_clear_reuse | ~46 | |
| list_insert_mid | ~96 | |
| substring | ~127 | |

```bash
./scripts/run-microbenches.sh
# or
dream --release run tests/bench/microbenches.dream
```
