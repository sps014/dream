# Microbench baseline — tiered GC

Recorded with `dream --release run tests/bench/microbenches.dream` after the ARC→GC
cutover (`docs/compiler/12-tiered-gc.md`). Absolute values vary by host — use relative
deltas. Re-run via `./scripts/run-microbenches.sh`.

## Post-Gen0 snapshot (2026-08-12, native wasmtime)

Nursery = 256 KiB. Gen0 bump + evacuate enabled; mutator rooting (call/alloc reload,
ref-arg spill, `$__obj_rg`) plus `GC_EPOCH_ADDR` so reloads no-op when no collection ran.

| Bench | ns_per_op | notes |
|-------|----------:|-------|
| string_eq | 10 | |
| list_push | 15 | |
| alloc_churn | 23 | |
| list_clear_reuse | 27 | ARC-era ~9 |
| gc_locals | 57 | |
| scratch_arena | 79 | |
| string_concat | 97 | |
| map_clear_reuse | 102 | ARC-era ~46 |
| map_get_set | 124 | |
| list_insert_mid | 125 | ARC-era ~96 |
| substring | 144 | ARC-era ~127 |
| string_builder | 355 | |
| char_scan | 3.6k | |
| regex_find | 60k | |

Short-lived allocs are back on the nursery path. Clear/reuse and map rows still trail
ARC (retain/release elision + prompt freelist), but are ~2× faster than the
all-Gen1 fallback that shipped while Gen0 was disabled.

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
