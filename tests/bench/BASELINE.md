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

Raw logs: `out_native_baseline.txt`, `out_native_after.txt`

```bash
./scripts/run-microbenches.sh
# or
dream --release run tests/bench/microbenches.dream
```
