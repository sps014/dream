# Microbench baseline — tiered GC

Recorded with `dream --release run tests/bench/microbenches.dream` after the ARC→GC
cutover (`docs/internals/12-tiered-gc.md`). Absolute values vary by host — use relative
deltas. Re-run via `./scripts/run-microbenches.sh`.

## Snapshot (2026-08-13, native wasmtime, 3 consecutive runs)

Nursery = 2 MiB. Gen0 skips the old-space walk unless the remset overflowed; skips the
dead-nursery walk unless `weak` / `del` / `js` fields are live. Mutator bump/epoch/request
are WASM globals (no linear-memory load on the malloc fast path).

Warm runs (2–3) vs ARC-era reference:

| Bench | ARC | run2 | run3 | vs ARC |
|-------|----:|-----:|-----:|--------|
| list_clear_reuse | ~9 | 8 | 7 | faster |
| map_clear_reuse | ~46 | 38 | 33 | faster |
| list_insert_mid | ~96 | 123 | 105 | close (run3) |
| substring | ~127 | 184 | 146 | close (run3) |

Full run 3 (quiet, after warmup):

| Bench | ns_per_op |
|-------|----------:|
| list_push | 7 |
| list_clear_reuse | 7 |
| string_eq | 14 |
| alloc_churn | 24 |
| map_clear_reuse | 33 |
| gc_locals | 53 |
| scratch_arena | 70 |
| map_get_set | 79 |
| char_scan | 756 |
| string_concat | 110 |
| list_insert_mid | 105 |
| substring | 146 |
| string_builder | 355 |
| regex_find | 8.2k |

### Prior snapshot (2026-08-13, regex NFA pass)

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
| regex_find | 9.6k | |

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
