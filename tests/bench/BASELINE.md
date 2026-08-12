# Microbench baseline — tiered GC

Recorded with `dream --release run tests/bench/microbenches.dream` after the ARC→GC
cutover (`docs/compiler/12-tiered-gc.md`). Absolute values vary by host — use relative
deltas. Re-run via `./scripts/run-microbenches.sh`.

## Post-GC snapshot (2026-08-12, native wasmtime)

Nursery = 256 KiB. Numbers from a release build on the GC branch (partial suite: ScratchArena
bench still overflows under Gen0 — see open items). Sane i64 timers on short benches;
pathological negative `ns_total` on heavy string-churn benches indicates Stopwatch/`long`
field survival under Gen0 still needs hardening before trusting those rows.

| Bench | ns_per_op (approx) | notes |
|-------|-------------------:|-------|
| string_eq | ~10 | |
| list_push | ~15 | |
| list_clear_reuse | ~22 | |
| alloc_churn | ~42 | |
| map_clear_reuse | ~78 | |
| map_get_set | ~92 | |
| list_insert_mid | ~122 | |
| char_scan | ~3.9k | |

### ARC-era reference (historical)

| Bench | ns_per_op (approx) | notes |
|-------|-------------------:|-------|
| list_clear_reuse | ~9 | ARC + clear reuse |
| map_clear_reuse | ~46 | |
| list_insert_mid | ~96 | |
| substring | ~127 | |

## Open bench / Gen0 items

- **Gen0 copying disabled** in `$gc_alloc` (all managed allocs → Gen1/LOH) until evacuate is
  trustworthy; re-enable and kill this bullet when Map/generator stress is green under a
  256 KiB nursery.
- Stopwatch `long` fields / heavy `gc_locals`+`substring` timings can wrap or go negative —
  treat those benches as unreliable until fixed.
- `ScratchArena` microbench may still overflow under collect pressure.

```bash
./scripts/run-microbenches.sh
# or
dream --release run tests/bench/microbenches.dream
```
