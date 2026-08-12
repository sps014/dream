# Microbench baseline — tiered GC

Recorded with `dream --release run tests/bench/microbenches.dream` after the ARC→GC
cutover (`docs/compiler/12-tiered-gc.md`). Absolute values vary by host — use relative
deltas. Re-run via `./scripts/run-microbenches.sh`.

## Snapshot (2026-08-12, native wasmtime)

Nursery = 1 MiB. `$malloc` honors `GC_REQUEST` and old-space budget. Remset last-slot
skip + 512-byte cards. Sequential `$char_at` walk cache; `string.compare` is UTF-8
byte order. List/Map/Set keep a constructor-time blank slot (no per-pop alloc).

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
| **char_scan** | **867** | was ~4.1k (sequential UTF-8 cache) |
| regex_find | 36k | |

`char_scan` is the large win this pass (~4.7×). Collection ns/op moved a little with
the extra blank/defaults buffers; treat those as noise vs the previous snapshot.

### Prior snapshot (same day, pre string/remset pass)

| Bench | ns_per_op | notes |
|-------|----------:|-------|
| list_push | 6 | |
| list_clear_reuse | 6 | ARC-era ~9 |
| string_eq | 11 | |
| alloc_churn | 23 | |
| map_clear_reuse | 36 | ARC-era ~46 |
| scratch_arena | 59 | |
| gc_locals | 57 | |
| map_get_set | 81 | |
| list_insert_mid | 91 | |
| string_concat | 112 | |
| substring | 131 | |
| string_builder | 395 | |
| char_scan | 4.1k | |
| regex_find | 31k | |

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
