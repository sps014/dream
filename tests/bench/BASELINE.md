# Microbench baseline (pre–Phase 1) and post-campaign snapshot

Recorded with `dream --release run tests/bench/microbenches.dream`. Absolute values vary by
host — use relative deltas. `$string_eq` uses a word-wise compare (no portable `memory.compare`
in our WASM assembler target).

## Before (campaign start)

| Bench | ns_per_op (approx) |
|-------|-------------------:|
| substring | 2547 |
| list_insert_mid | 543 |
| map_clear_reuse | 106 |
| string_builder | 405 |

## After (ARC + stdlib opts)

| Bench | ns_per_op (approx) | vs before |
|-------|-------------------:|----------:|
| substring | ~180 | ~14× faster |
| list_insert_mid | ~170 | ~3× faster |
| map_clear_reuse | ~65 | ~1.6× faster |

## After (beat-GC levers: clear reuse / SROA ctor expand / ScratchArena / regex SOA)

| Bench | ns_per_op (approx) | notes |
|-------|-------------------:|-------|
| list_clear_reuse | ~9 | in-place clear, capacity kept |
| map_clear_reuse | ~46 | states/slots zeroed in place |
| scratch_arena | ~41 | bump + reset vs alloc_churn ~22 (different shape) |
| regex_find | ~33k | SOA queues + reused mark/caps buffers |
| list_insert_mid | ~96 | improved vs prior ~158 |
| substring | ~127 | |

Raw log: `out/native.txt` (from `./scripts/run-microbenches.sh`).


```bash
./scripts/run-microbenches.sh
# or
dream --release run tests/bench/microbenches.dream
```
